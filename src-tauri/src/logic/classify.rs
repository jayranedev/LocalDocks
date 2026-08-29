//! The Developer classifier: registry + observable data -> one explainable
//! verdict.
//!
//! # Contract
//!
//! Pure. No system call, no I/O, no clock, no global state. It is handed an
//! executable name and, when one is available, a command line — both of which
//! the sampler already collected — and it returns a verdict and the sentence
//! that justifies it. That is the whole interface, which is what makes every
//! rule below testable without Windows.
//!
//! # The rules, in order
//!
//! First match wins. There is no scoring, no weighting and no threshold; the
//! order below *is* the algorithm, and it is short enough to read in full:
//!
//!   1. The executable is in the exclusion table -> **System**.
//!   2. The executable is a registered *dedicated* development program ->
//!      **Developer**.
//!   3. The executable is a registered *runtime* and its command line contains
//!      a registered signature -> **Developer**, naming the signature.
//!   4. The executable is a registered runtime and its command line contains no
//!      registered signature (or could not be read) -> **Unknown**.
//!   5. Anything else -> **Unknown**.
//!
//! Every branch produces a reason naming the rule that fired, so any
//! classification the user disagrees with can be traced to one table entry.
//!
//! # What is deliberately not consulted
//!
//! * **The port.** Not the number, not a range, not a "known ports" table. A
//!   service is not developer-relevant because it listens on 3000, and one on
//!   61123 is not disqualified. The port is what the model explains, never an
//!   input to it.
//! * **The address.** A server on `0.0.0.0:8000` is exactly as relevant as one
//!   on `127.0.0.1:8000`. Filtering by loopback would hide the bindings a
//!   developer most needs to notice.
//! * **Ancestry.** No parent, no child, no tree walk. `explorer.exe` starts
//!   everything, so one hop from a service reaches unrelated siblings and two
//!   hops reach the session. A process is classified on what it is, not on what
//!   started it.
//! * **Resource use.** CPU and memory say nothing about relevance.

use std::collections::HashMap;

use crate::logic::registry::{self, Evidence, SignatureKind};
use crate::models::{ProcessId, Relevance, Service};

/// A verdict and the sentence that justifies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    pub relevance: Relevance,
    /// One sentence, written to be checked. It always names the concrete thing
    /// that decided the outcome — a registry entry, a matched token, or the
    /// absence of both — so a wrong verdict points at the rule that produced
    /// it rather than at "the classifier".
    pub reason: String,
}

/// Classify one service.
///
/// `process_name` is the executable file name as Windows reports it
/// (`node.exe`). `command_line` is the process's full command line when it
/// could be read, and `None` when it could not — a distinction the reason keeps
/// visible rather than collapsing into "no match".
pub fn classify(process_name: &str, command_line: Option<&str>) -> Classification {
    let stem = executable_stem(process_name);

    // Rule 1. Checked first so no signature can ever promote an excluded
    // program. This is a guard, not the mechanism: the default is already
    // `Unknown`, which Developer mode also hides.
    if let Some(e) = registry::excluded(&stem) {
        return Classification {
            relevance: Relevance::System,
            reason: format!("{} is {}.", e.display, e.category),
        };
    }

    if stem.is_empty() {
        return Classification {
            relevance: Relevance::Unknown,
            reason: "This process has no readable executable name, so it cannot \
                     be classified."
                .to_string(),
        };
    }

    let Some(program) = registry::program(&stem) else {
        // Rule 5. Not a claim that this is a system process — only that the
        // registry has never heard of it. The registry is not exhaustive and
        // does not pretend to be.
        return Classification {
            relevance: Relevance::Unknown,
            reason: format!(
                "{stem} is in neither the developer nor the system registry, \
                 so it is not classified."
            ),
        };
    };

    // Rule 2.
    if program.evidence == Evidence::Dedicated {
        return Classification {
            relevance: Relevance::Developer,
            reason: format!("{} is {}.", program.display, program.category),
        };
    }

    // Rules 3 and 4: a general-purpose runtime, which the name alone can never
    // settle. This is the branch that stops "it is a Node process" from meaning
    // "it is a development service".
    let Some(line) = command_line else {
        return Classification {
            relevance: Relevance::Unknown,
            reason: format!(
                "{} is {}; its command line could not be read, so there is \
                 nothing to classify it by.",
                program.display, program.category
            ),
        };
    };

    match best_signature(line) {
        // Rule 3.
        Some(hit) => Classification {
            relevance: Relevance::Developer,
            reason: match hit.kind {
                SignatureKind::Tool => {
                    format!(
                        "{} launched with the {} signature.",
                        program.display, hit.display
                    )
                }
                SignatureKind::Verb => format!(
                    "{} command line carries the development token \"{}\".",
                    program.display, hit.token
                ),
                SignatureKind::Workspace => {
                    format!("{} launched from {}.", program.display, hit.display)
                }
            },
        },
        // Rule 4.
        None => Classification {
            relevance: Relevance::Unknown,
            reason: format!(
                "{} is {}; nothing in its command line matched a registered \
                 development signature.",
                program.display, program.category
            ),
        },
    }
}

/// Classify every service in place.
///
/// The second half of the tick: `logic::service` decided *what is a service*
/// from observation, and this decides *whether it is mine* from the registry.
/// Split in two because the first half must stay syscall-free and the second
/// half needs a command line, which only Windows can supply.
///
/// `command_lines` is keyed by process identity — `{pid}-{startedAt}`, not a
/// bare PID — so a recycled PID can never inherit the previous process's
/// command line and, with it, its classification. A missing key means the
/// command line could not be read; a `None` value means the same thing and is
/// how a cached failure is remembered without retrying it every tick.
pub fn apply(services: &mut [Service], command_lines: &HashMap<ProcessId, Option<String>>) {
    for service in services {
        let line = command_lines.get(&service.id).and_then(|v| v.as_deref());
        let verdict = classify(&service.process_name, line);
        service.relevance = verdict.relevance;
        service.relevance_reason = verdict.reason;
    }
}

/// Which services need a command line read for them.
///
/// The bounded half of the tier-2 amendment in docs/ARCHITECTURE.md § 4: only
/// services are asked, never all ~400 processes, and only those whose
/// classification could actually change as a result. A dedicated program is
/// already decided by its name and an excluded one is already refused, so
/// neither is worth a handle.
pub fn needs_command_line(service: &Service) -> bool {
    let stem = executable_stem(&service.process_name);
    if registry::excluded(&stem).is_some() {
        return false;
    }
    matches!(
        registry::program(&stem),
        Some(p) if p.evidence == Evidence::Runtime
    )
}

/// `node.exe` -> `node`, lowercased for comparison against the registry.
///
/// Only a trailing `.exe` is removed, and only when something is left over:
/// `my.app.exe` is `my.app`, and a file literally named `.exe` keeps its name.
fn executable_stem(process_name: &str) -> String {
    let trimmed = process_name.trim();
    let stem = match trimmed.len().checked_sub(4) {
        Some(cut) if trimmed[cut..].eq_ignore_ascii_case(".exe") && cut > 0 => &trimmed[..cut],
        _ => trimmed,
    };
    stem.to_ascii_lowercase()
}

/// The strongest registered signature in a command line.
///
/// Strength is `Tool` > `Verb` > `Workspace`. Ranking rather than
/// first-match-wins so that `node .../node_modules/.bin/vite` is attributed to
/// Vite — the specific, checkable reason — rather than to the directory it
/// happened to live in. Ties keep the earliest token, because a later
/// equal-ranked match never displaces an existing one. Deterministic either
/// way: the same command line always yields the same sentence.
fn best_signature(command_line: &str) -> Option<&'static registry::Signature> {
    let mut best: Option<&'static registry::Signature> = None;

    for token in tokenize(command_line) {
        let Some(signature) = registry::signature(&token) else {
            continue;
        };
        if best.map_or(true, |current| rank(signature.kind) < rank(current.kind)) {
            best = Some(signature);
        }
    }

    best
}

fn rank(kind: SignatureKind) -> u8 {
    match kind {
        SignatureKind::Tool => 0,
        SignatureKind::Verb => 1,
        SignatureKind::Workspace => 2,
    }
}

/// Split a command line into whole tokens.
///
/// **This is the part that has to be right.** Substring matching against a raw
/// command line is the obvious implementation and it is wrong, in ways that are
/// not hypothetical — both of these are real command lines observed on a
/// development machine:
///
/// ```text
/// --utility-sub-type=node.mojom.NodeService
/// --inspect-port=0
/// ```
///
/// A substring search for `node` matches the first; a search for `inspect`
/// matches the second. Neither process is development tooling. Splitting into
/// tokens and comparing whole tokens makes both cases impossible rather than
/// merely unlikely.
///
/// Separators are whitespace, quotes, both path separators, and the argument
/// punctuation Windows programs use (`=`, `,`, `;`, `:`). Then each token has
/// its leading dashes and its trailing script extension removed, so
/// `--watch` is `watch` and `vite.js` is `vite`.
///
/// `.` is deliberately *not* a separator. Splitting on it would turn
/// `node.mojom.NodeService` back into a bare `node`, which is the exact failure
/// this function exists to prevent.
fn tokenize(command_line: &str) -> impl Iterator<Item = String> + '_ {
    command_line
        .split(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '"' | '\'' | '\\' | '/' | '=' | ',' | ';' | ':' | '(' | ')'
                )
        })
        .filter(|t| !t.is_empty())
        .map(normalize)
        .filter(|t| !t.is_empty())
}

/// One raw token to its comparable form.
fn normalize(token: &str) -> String {
    let without_flag = token.trim_start_matches('-');
    let stem = strip_script_extension(without_flag);
    stem.to_ascii_lowercase()
}

/// Remove a trailing extension that names a *file type*, not a program.
///
/// `vite.js` and `manage.py` must compare equal to `vite` and `manage`. The
/// list is closed on purpose: an open rule of "drop anything after the last
/// dot" would reduce `node.mojom.NodeService` to `node.mojom`, and eventually
/// somebody would extend it to the last segment and reintroduce the bug.
fn strip_script_extension(token: &str) -> &str {
    const EXTENSIONS: &[&str] = &[
        ".js", ".mjs", ".cjs", ".ts", ".mts", ".cts", ".jsx", ".tsx", ".py", ".rb", ".php", ".exe",
        ".cmd", ".bat", ".ps1", ".sh", ".jar",
    ];
    for ext in EXTENSIONS {
        if token.len() > ext.len() {
            let cut = token.len() - ext.len();
            if token[cut..].eq_ignore_ascii_case(ext) {
                return &token[..cut];
            }
        }
    }
    token
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict(name: &str, line: Option<&str>) -> Relevance {
        classify(name, line).relevance
    }

    fn reason(name: &str, line: Option<&str>) -> String {
        classify(name, line).reason
    }

    // ------------------------------------------------- 1. dedicated programs

    #[test]
    fn a_dedicated_development_program_is_developer_on_its_name_alone() {
        let c = classify("mongod.exe", None);
        assert_eq!(c.relevance, Relevance::Developer);
        assert_eq!(c.reason, "MongoDB is a database server.");
    }

    #[test]
    fn dedicated_programs_do_not_need_a_command_line() {
        for name in [
            "postgres.exe",
            "redis-server.exe",
            "dockerd.exe",
            "nginx.exe",
        ] {
            assert_eq!(
                verdict(name, None),
                Relevance::Developer,
                "{name} should classify without a command line"
            );
        }
    }

    // ---------------------------------------------------- 2. runtime + tool

    #[test]
    fn a_runtime_with_a_tool_signature_is_developer_and_names_the_tool() {
        // The real command line of the Vite dev server running on the
        // development machine this registry was built against.
        let line =
            r#""node"   "C:\Desktop\BRO.UNIVERSITY\node_modules\.bin\\..\vite\bin\vite.js" "#;
        let c = classify("node.exe", Some(line));
        assert_eq!(c.relevance, Relevance::Developer);
        assert_eq!(c.reason, "Node.js launched with the Vite signature.");
    }

    #[test]
    fn a_named_tool_outranks_the_directory_it_was_found_in() {
        // Both `node_modules` and `vite` match. The specific one must win, so
        // the reason is the checkable one.
        let c = classify("node.exe", Some(r"node C:\p\node_modules\vite\bin\vite.js"));
        assert!(c.reason.contains("Vite"), "got: {}", c.reason);
        assert!(!c.reason.contains("node_modules"), "got: {}", c.reason);
    }

    #[test]
    fn common_runtimes_and_tools_classify_across_ecosystems() {
        for (name, line, expect) in [
            (
                "python.exe",
                "python -m uvicorn app:app --port 8000",
                "Uvicorn",
            ),
            (
                "python.exe",
                r"C:\p\.venv\Scripts\python.exe manage.py runserver",
                "Django",
            ),
            ("ruby.exe", "ruby bin/rails server", "Rails"),
            ("php.exe", "php artisan serve", "Laravel Artisan"),
            ("dotnet.exe", "dotnet watch --project Api.csproj", "watch"),
            (
                "java.exe",
                r"java -jar C:\p\gradle\wrapper\gradle-wrapper.jar bootRun",
                "Gradle",
            ),
            ("node.exe", "node node_modules/.bin/next dev", "Next.js"),
            ("bun.exe", "bun run dev", "run"),
        ] {
            let c = classify(name, Some(line));
            assert_eq!(c.relevance, Relevance::Developer, "{line}");
            assert!(c.reason.contains(expect), "{line} -> {}", c.reason);
        }
    }

    // --------------------------------------------- 3. the substring failures

    /// The single most important test in this file. This is a real VS Code
    /// helper command line, and a substring search for `node` matches it.
    #[test]
    fn a_command_line_mentioning_node_inside_a_longer_token_is_not_a_match() {
        let line = "--type=utility --utility-sub-type=node.mojom.NodeService --lang=en-US";
        // Even given a runtime executable, which is the generous case.
        assert_eq!(verdict("node.exe", Some(line)), Relevance::Unknown);
    }

    /// Also real, and the reason `--inspect` is not a registered signature.
    #[test]
    fn inspect_port_is_not_a_development_signature() {
        let line = "--inspect-port=0 --dns-result-order=ipv4first";
        assert_eq!(verdict("node.exe", Some(line)), Relevance::Unknown);
    }

    /// `NVIDIA Web Helper.exe index.js` is a real listening process. It runs a
    /// Node script and must still never be Developer.
    #[test]
    fn a_vendor_helper_running_a_node_script_is_not_developer() {
        let c = classify(
            "NVIDIA Web Helper.exe",
            Some(r#""C:\...\NVIDIA Web Helper.exe" index.js"#),
        );
        assert_eq!(c.relevance, Relevance::System);
        assert!(c.reason.contains("NVIDIA"), "got: {}", c.reason);
    }

    #[test]
    fn signature_tokens_never_match_as_substrings_of_other_words() {
        // `invite` contains `vite`; `runtime` contains `run`; `observe`
        // contains `serve`; `restart` contains `start`.
        for line in [
            "node invite.js",
            "node runtime.js",
            "node observer.js",
            "node restarter.js",
        ] {
            assert_eq!(
                verdict("node.exe", Some(line)),
                Relevance::Unknown,
                "{line}"
            );
        }
    }

    // ------------------------------------------------------- 4. the unknowns

    #[test]
    fn a_runtime_with_no_matching_signature_is_unknown_not_developer() {
        let c = classify("node.exe", Some(r"node C:\vendor\helper.js"));
        assert_eq!(c.relevance, Relevance::Unknown);
        assert!(
            c.reason.contains("nothing in its command line matched"),
            "got: {}",
            c.reason
        );
    }

    #[test]
    fn a_runtime_whose_command_line_could_not_be_read_is_unknown_and_says_so() {
        let c = classify("node.exe", None);
        assert_eq!(c.relevance, Relevance::Unknown);
        assert!(c.reason.contains("could not be read"), "got: {}", c.reason);
        // Distinguishable from the "read but matched nothing" case.
        assert_ne!(c.reason, reason("node.exe", Some("node helper.js")));
    }

    #[test]
    fn an_unregistered_program_is_unknown_and_the_registry_does_not_claim_otherwise() {
        let c = classify("SomeInternalTool.exe", Some("--serve --dev"));
        assert_eq!(c.relevance, Relevance::Unknown);
        assert!(c.reason.contains("neither"), "got: {}", c.reason);
    }

    /// Editors hold real listening sockets and are in neither table. They must
    /// be Unknown — not Developer, and not falsely labelled System.
    #[test]
    fn an_editor_is_unknown_rather_than_developer_or_system() {
        for editor in ["Code.exe", "cursor.exe", "idea64.exe", "devenv.exe"] {
            let c = classify(editor, Some("--type=utility"));
            assert_eq!(c.relevance, Relevance::Unknown, "{editor}");
        }
    }

    // ----------------------------------------------------- 5. the exclusions

    /// The list the correction pass named explicitly. None of these may be
    /// Developer, whatever they listen on.
    #[test]
    fn consumer_and_system_software_is_never_developer() {
        for name in [
            "chrome.exe",
            "brave.exe",
            "msedge.exe",
            "firefox.exe",
            "Spotify.exe",
            "SpotifyLauncher.exe",
            "iCloudDrive.exe",
            "iCloudPhotos.exe",
            "iCloudHome.exe",
            "iCloudCKKS.exe",
            "APSDaemon.exe",
            "AppleMobileDeviceProcess.exe",
            "nvcontainer.exe",
            "NVIDIA Web Helper.exe",
            "steam.exe",
            "lghub_updater.exe",
            "claude.exe",
            "svchost.exe",
            "lsass.exe",
            "spoolsv.exe",
            "System",
        ] {
            let c = classify(name, Some("--serve --dev --watch --run vite next"));
            assert_eq!(
                c.relevance,
                Relevance::System,
                "{name} must be excluded even with a command line full of signatures"
            );
        }
    }

    #[test]
    fn exclusion_is_checked_before_any_signature_can_apply() {
        // The generous adversarial case: an excluded program whose command
        // line is nothing but development signatures.
        let c = classify(
            "chrome.exe",
            Some("vite next uvicorn rails dev serve node_modules"),
        );
        assert_eq!(c.relevance, Relevance::System);
    }

    // ------------------------------------------------------ 6. the invariants

    #[test]
    fn a_port_number_never_appears_as_an_input_or_changes_a_verdict() {
        // The same process on four different ports classifies identically —
        // there is no port in the signature at all.
        let base = classify("node.exe", Some("node vite.js"));
        for port in ["3000", "5173", "8080", "61123"] {
            let c = classify("node.exe", Some(&format!("node vite.js --port {port}")));
            assert_eq!(c, base, "port {port} changed the classification");
        }
        // And a bare port number in a command line classifies nothing.
        assert_eq!(
            verdict("someapp.exe", Some("--port 3000")),
            Relevance::Unknown
        );
        assert_eq!(
            verdict("someapp.exe", Some("--port 8080")),
            Relevance::Unknown
        );
    }

    #[test]
    fn the_address_a_service_binds_never_changes_a_verdict() {
        let a = classify("node.exe", Some("node vite.js --host 127.0.0.1"));
        let b = classify("node.exe", Some("node vite.js --host 0.0.0.0"));
        assert_eq!(a, b);
    }

    #[test]
    fn classification_is_deterministic() {
        let line = r"node C:\p\node_modules\.bin\vite --port 5173 --host";
        let first = classify("node.exe", Some(line));
        for _ in 0..50 {
            assert_eq!(classify("node.exe", Some(line)), first);
        }
    }

    #[test]
    fn every_verdict_carries_a_reason_that_is_a_sentence() {
        for (name, line) in [
            ("mongod.exe", None),
            ("node.exe", Some("node vite.js")),
            ("node.exe", Some("node helper.js")),
            ("node.exe", None),
            ("chrome.exe", None),
            ("mystery.exe", None),
        ] {
            let c = classify(name, line);
            assert!(!c.reason.trim().is_empty(), "{name} produced no reason");
            assert!(c.reason.ends_with('.'), "{name}: {}", c.reason);
            assert!(c.reason.len() > 20, "{name}: {}", c.reason);
        }
    }

    #[test]
    fn names_are_matched_case_insensitively_and_with_or_without_the_extension() {
        assert_eq!(verdict("MONGOD.EXE", None), Relevance::Developer);
        assert_eq!(verdict("mongod", None), Relevance::Developer);
        assert_eq!(verdict("  mongod.exe  ", None), Relevance::Developer);
    }

    #[test]
    fn an_empty_or_unreadable_name_is_unknown_rather_than_anything_else() {
        assert_eq!(verdict("", None), Relevance::Unknown);
        assert_eq!(verdict("   ", Some("vite")), Relevance::Unknown);
    }

    #[test]
    fn an_empty_command_line_is_treated_as_no_signature() {
        assert_eq!(verdict("node.exe", Some("")), Relevance::Unknown);
        assert_eq!(verdict("node.exe", Some("   ")), Relevance::Unknown);
    }

    // ---------------------------------------------------- 7. the tick helpers

    fn service(name: &str) -> Service {
        Service {
            id: crate::models::make_process_id(8420, "2026-08-28T09:00:00.000Z"),
            label: "x:5173".into(),
            framework: None,
            process_name: name.into(),
            pid: 8420,
            parent_pid: 1,
            cpu_percent: 0.0,
            memory_bytes: 0,
            thread_count: 1,
            started_at: "2026-08-28T09:00:00.000Z".into(),
            uptime_seconds: 0.0,
            endpoints: Vec::new(),
            status: crate::models::ServiceStatus::Running,
            relevance: Relevance::Unknown,
            relevance_reason: String::new(),
        }
    }

    #[test]
    fn only_general_purpose_runtimes_are_worth_a_command_line_read() {
        // Already decided by name: no handle needed.
        assert!(!needs_command_line(&service("mongod.exe")));
        assert!(!needs_command_line(&service("chrome.exe")));
        assert!(!needs_command_line(&service("Code.exe")));
        assert!(!needs_command_line(&service("mystery.exe")));
        // Undecidable without one.
        assert!(needs_command_line(&service("node.exe")));
        assert!(needs_command_line(&service("python.exe")));
        assert!(needs_command_line(&service("java.exe")));
    }

    #[test]
    fn apply_fills_in_every_service_and_leaves_no_empty_reason() {
        let mut services = vec![
            service("node.exe"),
            service("chrome.exe"),
            service("mongod.exe"),
        ];
        services[1].id = crate::models::make_process_id(1, "t");
        services[2].id = crate::models::make_process_id(2, "t");

        let mut lines = HashMap::new();
        lines.insert(services[0].id.clone(), Some("node vite.js".to_string()));

        apply(&mut services, &lines);

        assert_eq!(services[0].relevance, Relevance::Developer);
        assert_eq!(services[1].relevance, Relevance::System);
        assert_eq!(services[2].relevance, Relevance::Developer);
        assert!(services.iter().all(|s| !s.relevance_reason.is_empty()));
    }

    #[test]
    fn apply_keys_command_lines_by_identity_so_a_recycled_pid_cannot_inherit_one() {
        let mut services = vec![service("node.exe")];
        // The same PID, a later start: a different identity.
        services[0].id = crate::models::make_process_id(8420, "2026-08-28T10:00:00.000Z");

        let mut lines = HashMap::new();
        lines.insert(
            crate::models::make_process_id(8420, "2026-08-28T09:00:00.000Z"),
            Some("node vite.js".to_string()),
        );

        apply(&mut services, &lines);

        assert_eq!(
            services[0].relevance,
            Relevance::Unknown,
            "the earlier process's command line must not be reused"
        );
    }

    #[test]
    fn a_cached_read_failure_is_treated_as_no_command_line() {
        let mut services = vec![service("node.exe")];
        let mut lines = HashMap::new();
        lines.insert(services[0].id.clone(), None);
        apply(&mut services, &lines);
        assert_eq!(services[0].relevance, Relevance::Unknown);
        assert!(services[0].relevance_reason.contains("could not be read"));
    }

    // -------------------------------------------------------- the tokenizer

    #[test]
    fn tokenize_splits_paths_quotes_and_argument_punctuation() {
        let tokens: Vec<String> =
            tokenize(r#""node" "C:\p\node_modules\.bin\vite.js" --port=5173"#).collect();
        assert!(tokens.contains(&"node".to_string()));
        assert!(tokens.contains(&"node_modules".to_string()));
        assert!(tokens.contains(&"vite".to_string()), "{tokens:?}");
        assert!(tokens.contains(&"port".to_string()));
        assert!(tokens.contains(&"5173".to_string()));
    }

    #[test]
    fn tokenize_does_not_split_on_dots() {
        let tokens: Vec<String> = tokenize("--utility-sub-type=node.mojom.NodeService").collect();
        assert!(
            tokens.contains(&"node.mojom.nodeservice".to_string()),
            "{tokens:?}"
        );
        assert!(!tokens.contains(&"node".to_string()), "{tokens:?}");
    }

    #[test]
    fn only_closed_list_extensions_are_stripped() {
        assert_eq!(strip_script_extension("vite.js"), "vite");
        assert_eq!(strip_script_extension("manage.py"), "manage");
        assert_eq!(
            strip_script_extension("node.mojom.NodeService"),
            "node.mojom.NodeService"
        );
        assert_eq!(strip_script_extension(".js"), ".js");
    }
}
