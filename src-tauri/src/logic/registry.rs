//! The Developer Registry: the one place LocalDocks decides what "developer"
//! means.
//!
//! # Why a registry exists at all
//!
//! docs/ARCHITECTURE.md § 1 rejects allowlists for deciding what a *Service*
//! is, and that still holds: a Service is an observed fact — a process the user
//! owns holding a listening socket on a non-system port. Nothing in
//! `logic::service` looks at a name.
//!
//! Relevance is a different question, and it cannot be answered by observation
//! alone. "Is this service part of my development work?" has no syscall. The
//! honest options are a registry or a guess, and a guess that fails is
//! unexplainable. So this file exists, and it is deliberately the *only* file
//! that names a program.
//!
//! # What this is not
//!
//! * **Not a port table.** No entry anywhere in this file is a port number. A
//!   service on 3000 is not developer-relevant because it is on 3000; there is
//!   no "3000–9000 is developer" range and no known-port list. Ports are an
//!   output of the model, never an input.
//! * **Not ancestry.** A process is never developer-relevant because of what
//!   spawned it. `explorer.exe` starts everything; one hop from a service
//!   reaches unrelated siblings. The relationship is not evidence.
//! * **Not a score.** There is no weight, threshold or model. Classification is
//!   a first-match-wins walk down an ordered list of rules, and every outcome
//!   carries the rule that produced it.
//! * **Not exhaustive, and it does not claim to be.** The third outcome is
//!   `Unknown`, and it is the default. A service this file has never heard of
//!   is reported as unrecognised rather than guessed at in either direction.
//!
//! # Versioning
//!
//! `REGISTRY_VERSION` changes whenever an entry is added, removed or
//! reclassified. It ships in the snapshot so a bug report can say which
//! registry produced a classification, and so the docs can pin a number.

/// Bumped on every change to the tables below. See the module docs.
pub const REGISTRY_VERSION: u32 = 1;

// ---------------------------------------------------------------- executables

/// What kind of evidence an executable's *name* provides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    /// The program exists to serve development and does essentially nothing
    /// else. `mongod` is not a general-purpose binary that might be doing
    /// something unrelated — running it *is* the development activity. The name
    /// alone is sufficient.
    Dedicated,
    /// A general-purpose runtime. `node.exe` runs dev servers, but it also runs
    /// bundled helpers inside consumer applications, so the name alone proves
    /// nothing. These require a command-line signature; on their own they
    /// produce `Unknown`, never `Developer`.
    Runtime,
}

/// One registered developer executable.
#[derive(Debug, Clone, Copy)]
pub struct Program {
    /// Lowercase executable stem, with no `.exe`. Compared exactly, never as a
    /// substring: `node` must not match `nodejs-updater`.
    pub stem: &'static str,
    /// How this name is written in a reason string.
    pub display: &'static str,
    pub evidence: Evidence,
    /// Predicate phrase, used as `"{display} is {category}"` in a reason and
    /// as the grouping key in the validation report. It carries its own
    /// article so the sentence reads correctly.
    pub category: &'static str,
}

const fn dedicated(stem: &'static str, display: &'static str, category: &'static str) -> Program {
    Program {
        stem,
        display,
        evidence: Evidence::Dedicated,
        category,
    }
}

const fn runtime(stem: &'static str, display: &'static str) -> Program {
    Program {
        stem,
        display,
        evidence: Evidence::Runtime,
        category: "a general-purpose language runtime",
    }
}

/// Programs whose name is itself a development signature.
///
/// The bar for membership: **on a workstation, this binary running is the
/// development activity.** A database daemon qualifies. A language interpreter
/// does not, and lives in the runtime table below instead.
pub const DEDICATED: &[Program] = &[
    // -- Databases and data stores -----------------------------------------
    dedicated("mongod", "MongoDB", "a database server"),
    dedicated("mysqld", "MySQL", "a database server"),
    dedicated("mariadbd", "MariaDB", "a database server"),
    dedicated("postgres", "PostgreSQL", "a database server"),
    dedicated("pg_ctl", "PostgreSQL", "a database server"),
    dedicated("redis-server", "Redis", "a database server"),
    dedicated("memcached", "Memcached", "a database server"),
    dedicated("influxd", "InfluxDB", "a database server"),
    dedicated("clickhouse-server", "ClickHouse", "a database server"),
    dedicated("cockroach", "CockroachDB", "a database server"),
    dedicated("etcd", "etcd", "a database server"),
    dedicated("surreal", "SurrealDB", "a database server"),
    dedicated("neo4j", "Neo4j", "a database server"),
    dedicated("couchdb", "CouchDB", "a database server"),
    dedicated("cassandra", "Cassandra", "a database server"),
    // -- Search and vector stores ------------------------------------------
    dedicated("meilisearch", "Meilisearch", "a search engine"),
    dedicated("typesense-server", "Typesense", "a search engine"),
    dedicated("qdrant", "Qdrant", "a search engine"),
    dedicated("weaviate", "Weaviate", "a search engine"),
    dedicated("opensearch", "OpenSearch", "a search engine"),
    // -- Queues, brokers and object storage --------------------------------
    dedicated("nats-server", "NATS", "a message broker"),
    dedicated("rabbitmq-server", "RabbitMQ", "a message broker"),
    dedicated("mosquitto", "Mosquitto", "a message broker"),
    dedicated("minio", "MinIO", "an object storage server"),
    // -- Containers and local orchestration ---------------------------------
    dedicated("dockerd", "Docker", "a container runtime"),
    dedicated("docker", "Docker", "a container runtime"),
    dedicated(
        "com.docker.backend",
        "Docker Desktop",
        "a container runtime",
    ),
    dedicated("containerd", "containerd", "a container runtime"),
    dedicated("podman", "Podman", "a container runtime"),
    dedicated("minikube", "minikube", "a container runtime"),
    dedicated("k3s", "k3s", "a container runtime"),
    // -- Web servers and proxies used locally --------------------------------
    dedicated("nginx", "nginx", "a web server"),
    dedicated("httpd", "Apache httpd", "a web server"),
    dedicated("caddy", "Caddy", "a web server"),
    dedicated("traefik", "Traefik", "a web server"),
    // -- Tunnels and local endpoints -----------------------------------------
    dedicated("ngrok", "ngrok", "a local tunnel"),
    dedicated("cloudflared", "Cloudflare Tunnel", "a local tunnel"),
    // -- Backend platforms and local emulators -------------------------------
    dedicated("supabase", "Supabase", "a backend development platform"),
    dedicated("localstack", "LocalStack", "a local cloud emulator"),
    dedicated("azurite", "Azurite", "a local cloud emulator"),
    dedicated("adb", "Android Debug Bridge", "part of a mobile toolchain"),
    dedicated("emulator", "Android Emulator", "part of a mobile toolchain"),
    // -- Development servers with their own binary ---------------------------
    dedicated("hugo", "Hugo", "a static site generator"),
    dedicated("trunk", "Trunk", "a build tool"),
    dedicated("gradle", "Gradle", "a build tool"),
    dedicated("mailhog", "MailHog", "a development mail server"),
    dedicated("ollama", "Ollama", "a local model server"),
];

/// General-purpose runtimes. Registered so that a command-line signature can be
/// *attributed* to something, never so that the name alone can classify.
///
/// This table is what stops "it is a Node process" from meaning "it is a
/// development service". Consumer applications ship these binaries too.
pub const RUNTIMES: &[Program] = &[
    runtime("node", "Node.js"),
    runtime("deno", "Deno"),
    runtime("bun", "Bun"),
    runtime("python", "Python"),
    runtime("python3", "Python"),
    runtime("pythonw", "Python"),
    runtime("py", "Python"),
    runtime("ruby", "Ruby"),
    runtime("php", "PHP"),
    runtime("perl", "Perl"),
    runtime("java", "Java"),
    runtime("javaw", "Java"),
    runtime("dotnet", ".NET"),
    runtime("dart", "Dart"),
    runtime("elixir", "Elixir"),
    runtime("erl", "Erlang"),
];

// ------------------------------------------------------------------ signatures

/// What a matched command-line token proves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureKind {
    /// Names a specific development tool: `vite`, `uvicorn`, `rails`.
    Tool,
    /// A development command verb: `dev`, `serve`, `watch`. Weaker than a named
    /// tool, but it is still a registered token and still explainable — and it
    /// only ever applies to a process already identified as a dev runtime.
    Verb,
    /// A project-layout marker: `node_modules`, `.venv`. Proves the runtime was
    /// launched out of a project tree rather than out of an installed app.
    Workspace,
}

/// One registered command-line token.
#[derive(Debug, Clone, Copy)]
pub struct Signature {
    /// Lowercase token. Matched against whole tokens only — see
    /// `logic::classify::tokenize`. Substring matching is what would let
    /// `--utility-sub-type=node.mojom.NodeService` look like Node tooling.
    pub token: &'static str,
    pub display: &'static str,
    pub kind: SignatureKind,
}

const fn tool(token: &'static str, display: &'static str) -> Signature {
    Signature {
        token,
        display,
        kind: SignatureKind::Tool,
    }
}

const fn verb(token: &'static str) -> Signature {
    Signature {
        token,
        display: token,
        kind: SignatureKind::Verb,
    }
}

const fn workspace(token: &'static str, display: &'static str) -> Signature {
    Signature {
        token,
        display,
        kind: SignatureKind::Workspace,
    }
}

/// Command-line tokens that identify development work.
///
/// Every entry is the name a developer would recognise, because every entry has
/// to be printable in a reason a user can check against their own terminal.
pub const SIGNATURES: &[Signature] = &[
    // -- JavaScript and TypeScript -------------------------------------------
    tool("vite", "Vite"),
    tool("next", "Next.js"),
    tool("nuxt", "Nuxt"),
    tool("astro", "Astro"),
    tool("remix", "Remix"),
    tool("webpack", "webpack"),
    tool("webpack-dev-server", "webpack-dev-server"),
    tool("rollup", "Rollup"),
    tool("parcel", "Parcel"),
    tool("esbuild", "esbuild"),
    tool("nodemon", "nodemon"),
    tool("ts-node", "ts-node"),
    tool("tsx", "tsx"),
    tool("tsc", "TypeScript compiler"),
    tool("nest", "NestJS"),
    tool("ng", "Angular CLI"),
    tool("react-scripts", "react-scripts"),
    tool("vue-cli-service", "Vue CLI"),
    tool("sveltekit", "SvelteKit"),
    tool("gatsby", "Gatsby"),
    tool("eleventy", "Eleventy"),
    tool("storybook", "Storybook"),
    tool("vitest", "Vitest"),
    tool("jest", "Jest"),
    tool("playwright", "Playwright"),
    tool("cypress", "Cypress"),
    tool("expo", "Expo"),
    tool("metro", "Metro"),
    tool("wrangler", "Wrangler"),
    tool("http-server", "http-server"),
    tool("live-server", "live-server"),
    tool("browser-sync", "BrowserSync"),
    tool("json-server", "json-server"),
    tool("strapi", "Strapi"),
    tool("directus", "Directus"),
    tool("payload", "Payload"),
    tool("medusa", "Medusa"),
    tool("sanity", "Sanity"),
    tool("firebase", "Firebase CLI"),
    tool("netlify", "Netlify CLI"),
    tool("vercel", "Vercel CLI"),
    tool("amplify", "Amplify CLI"),
    tool("hardhat", "Hardhat"),
    tool("truffle", "Truffle"),
    tool("anvil", "Anvil"),
    tool("pm2", "PM2"),
    tool("nx", "Nx"),
    tool("turbo", "Turborepo"),
    tool("lerna", "Lerna"),
    tool("npm", "npm"),
    tool("pnpm", "pnpm"),
    tool("yarn", "Yarn"),
    tool("npx", "npx"),
    tool("concurrently", "concurrently"),
    // -- Python ---------------------------------------------------------------
    tool("django", "Django"),
    tool("manage", "Django manage.py"),
    tool("flask", "Flask"),
    tool("fastapi", "FastAPI"),
    tool("uvicorn", "Uvicorn"),
    tool("gunicorn", "Gunicorn"),
    tool("hypercorn", "Hypercorn"),
    tool("waitress-serve", "Waitress"),
    tool("daphne", "Daphne"),
    tool("celery", "Celery"),
    tool("streamlit", "Streamlit"),
    tool("gradio", "Gradio"),
    tool("jupyter", "Jupyter"),
    tool("jupyterlab", "JupyterLab"),
    tool("notebook", "Jupyter Notebook"),
    tool("mkdocs", "MkDocs"),
    tool("pytest", "pytest"),
    tool("mlflow", "MLflow"),
    tool("tensorboard", "TensorBoard"),
    tool("airflow", "Airflow"),
    tool("dagster", "Dagster"),
    tool("prefect", "Prefect"),
    tool("scrapy", "Scrapy"),
    tool("locust", "Locust"),
    // -- Ruby, PHP, JVM, .NET -------------------------------------------------
    tool("rails", "Rails"),
    tool("puma", "Puma"),
    tool("unicorn", "Unicorn"),
    tool("sidekiq", "Sidekiq"),
    tool("jekyll", "Jekyll"),
    tool("rackup", "Rack"),
    tool("artisan", "Laravel Artisan"),
    tool("symfony", "Symfony"),
    tool("composer", "Composer"),
    tool("spring", "Spring"),
    tool("springboot", "Spring Boot"),
    tool("quarkus", "Quarkus"),
    tool("micronaut", "Micronaut"),
    tool("tomcat", "Tomcat"),
    tool("jetty", "Jetty"),
    tool("gradlew", "Gradle wrapper"),
    tool("gradle", "Gradle"),
    tool("maven", "Maven"),
    tool("mvn", "Maven"),
    tool("aspnetcore", "ASP.NET Core"),
    // -- Development command verbs --------------------------------------------
    //
    // Only reachable for a registered runtime, so these never classify a
    // consumer application. `serve` in a browser's argument list cannot be
    // reached, because a browser is not in RUNTIMES.
    verb("dev"),
    verb("serve"),
    verb("server"),
    verb("watch"),
    verb("start"),
    verb("run"),
    verb("runserver"),
    verb("develop"),
    // -- Project layout --------------------------------------------------------
    workspace("node_modules", "a node_modules project directory"),
    workspace(".venv", "a Python virtual environment"),
    workspace("venv", "a Python virtual environment"),
    workspace("site-packages", "a Python site-packages directory"),
];

// ------------------------------------------------------------------ exclusions

/// One program that is definitively not a development service.
#[derive(Debug, Clone, Copy)]
pub struct Excluded {
    pub stem: &'static str,
    pub display: &'static str,
    pub category: &'static str,
}

const fn excl(stem: &'static str, display: &'static str, category: &'static str) -> Excluded {
    Excluded {
        stem,
        display,
        category,
    }
}

/// Programs that can never be classified `Developer`.
///
/// **This is a secondary guard, not the mechanism.** Relevance defaults to
/// `Unknown`, and `Unknown` is already hidden from Developer mode — so nothing
/// in this table is load-bearing for what the user sees. Removing the whole
/// list would not put Chrome into Developer mode; it would only move Chrome
/// from "System" to "Unrecognised" in the validation report.
///
/// It exists for two narrow reasons:
///
///   1. It makes the report honest. Saying *"Spotify is a media application"*
///      is a stronger, checkable claim than *"Spotify is not in the registry"*,
///      and the difference matters when someone is auditing a classification.
///   2. It is checked first, so a future signature can never accidentally
///      promote one of these — if a browser one day ships a `--serve` flag, it
///      still cannot become a development service.
///
/// It is therefore kept deliberately small and is not allowed to grow into the
/// primary defence. The rule for adding an entry: it must be a program that a
/// developer would be actively annoyed to see in Developer mode, and it must be
/// something that genuinely holds listening sockets. Entries that satisfy
/// neither are noise.
///
/// Note what is *absent*: editors and IDEs. VS Code, Cursor, the JetBrains
/// suite and Visual Studio hold real listening sockets, and they are neither
/// development services nor system infrastructure. They belong in neither
/// table, so they classify as `Unknown` — which is the truthful answer, and
/// keeps them out of Developer mode without asserting something false about
/// them. See docs/ARCHITECTURE.md.
pub const EXCLUDED: &[Excluded] = &[
    // -- Windows itself --------------------------------------------------------
    excl(
        "system",
        "the Windows kernel",
        "part of the operating system",
    ),
    excl("smss", "Session Manager", "part of the operating system"),
    excl(
        "csrss",
        "Client/Server Runtime",
        "part of the operating system",
    ),
    excl(
        "wininit",
        "Windows Start-Up",
        "part of the operating system",
    ),
    excl("winlogon", "Windows Logon", "part of the operating system"),
    excl(
        "services",
        "Service Control Manager",
        "part of the operating system",
    ),
    excl(
        "lsass",
        "Local Security Authority",
        "part of the operating system",
    ),
    excl(
        "svchost",
        "a Windows service host",
        "part of the operating system",
    ),
    excl(
        "spoolsv",
        "the Print Spooler",
        "part of the operating system",
    ),
    excl(
        "dwm",
        "the Desktop Window Manager",
        "part of the operating system",
    ),
    excl(
        "explorer",
        "Windows Explorer",
        "part of the operating system",
    ),
    excl(
        "searchindexer",
        "Windows Search",
        "part of the operating system",
    ),
    excl(
        "searchhost",
        "Windows Search",
        "part of the operating system",
    ),
    excl(
        "runtimebroker",
        "Runtime Broker",
        "part of the operating system",
    ),
    excl("dllhost", "COM Surrogate", "part of the operating system"),
    excl(
        "wmiprvse",
        "WMI Provider Host",
        "part of the operating system",
    ),
    excl("taskhostw", "Task Host", "part of the operating system"),
    excl(
        "sihost",
        "Shell Infrastructure Host",
        "part of the operating system",
    ),
    excl("msmpeng", "Microsoft Defender", "security software"),
    excl(
        "mssense",
        "Microsoft Defender for Endpoint",
        "security software",
    ),
    excl(
        "nissrv",
        "Microsoft Defender Network Inspection",
        "security software",
    ),
    excl(
        "trustedinstaller",
        "Windows Modules Installer",
        "part of the operating system",
    ),
    excl(
        "tiworker",
        "Windows Modules Installer Worker",
        "part of the operating system",
    ),
    excl(
        "usocoreworker",
        "Update Session Orchestrator",
        "part of the operating system",
    ),
    excl(
        "mousocoreworker",
        "Update Session Orchestrator",
        "part of the operating system",
    ),
    // -- Browsers and their helpers ---------------------------------------------
    excl("chrome", "Google Chrome", "a web browser"),
    excl("msedge", "Microsoft Edge", "a web browser"),
    excl(
        "msedgewebview2",
        "the Edge WebView2 runtime",
        "a web browser",
    ),
    excl("brave", "Brave", "a web browser"),
    excl("firefox", "Firefox", "a web browser"),
    excl("opera", "Opera", "a web browser"),
    excl("vivaldi", "Vivaldi", "a web browser"),
    excl("chromium", "Chromium", "a web browser"),
    // -- Apple services -----------------------------------------------------------
    excl("icloud", "iCloud", "a cloud sync client"),
    excl("iclouddrive", "iCloud Drive", "a cloud sync client"),
    excl("icloudphotos", "iCloud Photos", "a cloud sync client"),
    excl("icloudhome", "iCloud", "a cloud sync client"),
    excl("icloudckks", "iCloud sync", "a cloud sync client"),
    excl("icloudservices", "iCloud", "a cloud sync client"),
    excl(
        "apsdaemon",
        "the Apple Push notification service",
        "a cloud sync client",
    ),
    excl(
        "applemobiledevicelauncher",
        "Apple Mobile Device Support",
        "device support software",
    ),
    excl(
        "applemobiledeviceprocess",
        "Apple Mobile Device Support",
        "device support software",
    ),
    excl(
        "applemobiledeviceservice",
        "Apple Mobile Device Support",
        "device support software",
    ),
    excl("itunes", "iTunes", "a media application"),
    excl("mdnsresponder", "Bonjour", "device support software"),
    // -- Other cloud sync ---------------------------------------------------------
    excl("onedrive", "OneDrive", "a cloud sync client"),
    excl("dropbox", "Dropbox", "a cloud sync client"),
    excl("googledrivefs", "Google Drive", "a cloud sync client"),
    // -- GPU and peripheral vendors ------------------------------------------------
    excl(
        "nvcontainer",
        "the NVIDIA container service",
        "a hardware vendor helper",
    ),
    excl(
        "nvidia web helper",
        "the NVIDIA web helper",
        "a hardware vendor helper",
    ),
    excl(
        "nvdisplay.container",
        "the NVIDIA display container",
        "a hardware vendor helper",
    ),
    excl(
        "nvsphelper64",
        "an NVIDIA helper",
        "a hardware vendor helper",
    ),
    excl("nvidia share", "NVIDIA Share", "a hardware vendor helper"),
    excl("lghub", "Logitech G HUB", "a hardware vendor helper"),
    excl(
        "lghub_updater",
        "the Logitech G HUB updater",
        "a hardware vendor helper",
    ),
    excl(
        "lghub_agent",
        "the Logitech G HUB agent",
        "a hardware vendor helper",
    ),
    excl("icue", "Corsair iCUE", "a hardware vendor helper"),
    excl(
        "razer synapse service",
        "Razer Synapse",
        "a hardware vendor helper",
    ),
    // -- Consumer and communication applications --------------------------------------
    excl("spotify", "Spotify", "a media application"),
    excl(
        "spotifylauncher",
        "the Spotify launcher",
        "a media application",
    ),
    excl("steam", "Steam", "a game platform"),
    excl("steamwebhelper", "the Steam web helper", "a game platform"),
    excl(
        "epicgameslauncher",
        "the Epic Games Launcher",
        "a game platform",
    ),
    excl(
        "epicwebhelper",
        "the Epic Games web helper",
        "a game platform",
    ),
    excl("discord", "Discord", "a communication application"),
    excl("slack", "Slack", "a communication application"),
    excl("teams", "Microsoft Teams", "a communication application"),
    excl("ms-teams", "Microsoft Teams", "a communication application"),
    excl("zoom", "Zoom", "a communication application"),
    excl("whatsapp", "WhatsApp", "a communication application"),
    // The shipped executable is `WhatsApp.Root.exe`, so the stem carries the
    // `.Root` — a reminder that entries are matched whole and must be written
    // exactly as Windows reports the file name.
    excl("whatsapp.root", "WhatsApp", "a communication application"),
    excl("telegram", "Telegram", "a communication application"),
    excl(
        "claude",
        "the Claude desktop application",
        "a consumer desktop application",
    ),
    excl(
        "chatgpt",
        "the ChatGPT desktop application",
        "a consumer desktop application",
    ),
    excl("obs64", "OBS Studio", "a media application"),
];

// -------------------------------------------------------------------- lookups

/// Find a registered developer program by executable stem.
///
/// Exact comparison first, then the version-suffix retry described in
/// [`unversioned`]. Never a prefix and never a substring: `node-updater` is
/// not `node`.
pub fn program(stem: &str) -> Option<&'static Program> {
    let find = |s: &str| {
        DEDICATED
            .iter()
            .chain(RUNTIMES.iter())
            .find(|p| p.stem.eq_ignore_ascii_case(s))
    };
    find(stem).or_else(|| unversioned(stem).and_then(find))
}

/// Find an exclusion by executable stem.
pub fn excluded(stem: &str) -> Option<&'static Excluded> {
    let find = |s: &str| EXCLUDED.iter().find(|e| e.stem.eq_ignore_ascii_case(s));
    find(stem).or_else(|| unversioned(stem).and_then(find))
}

/// A stem with a trailing version dropped: `python3.12` -> `python`.
///
/// Windows ships several runtimes under versioned executable names. The
/// Microsoft Store build of Python installs as `python3.12.exe`, and that is
/// the name the process list shows — so an exact-match-only registry answers
/// "unrecognised" for one of the most common development runtimes on Windows.
/// This was found by running a real demo environment, not by reading the table.
///
/// The retry is deliberately narrow, and only ever runs *after* an exact match
/// has failed:
///
///   * Only a trailing run of digits and dots is removed, so `python3.12`
///     becomes `python` and `python2` becomes `python`.
///   * The result must still contain a letter, so a stem that is only digits
///     matches nothing.
///   * It must actually differ from the input, so a stem with no version costs
///     one comparison and nothing else.
///
/// `msedgewebview2` and `obs64` are in the exclusion table under their full
/// versioned names and match exactly, before this is ever reached — which is
/// why the retry runs second rather than as a normalisation step.
fn unversioned(stem: &str) -> Option<&str> {
    let trimmed = stem.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    if trimmed.len() == stem.len() || !trimmed.chars().any(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some(trimmed)
}

/// Find the signature a command-line token matches.
pub fn signature(token: &str) -> Option<&'static Signature> {
    SIGNATURES
        .iter()
        .find(|s| s.token.eq_ignore_ascii_case(token))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Two entries for one name means one of them silently never matches.
    #[test]
    fn every_table_has_unique_stems() {
        for (label, stems) in [
            (
                "developer executables",
                DEDICATED
                    .iter()
                    .chain(RUNTIMES.iter())
                    .map(|p| p.stem)
                    .collect::<Vec<_>>(),
            ),
            ("exclusions", EXCLUDED.iter().map(|e| e.stem).collect()),
            ("signatures", SIGNATURES.iter().map(|s| s.token).collect()),
        ] {
            let unique: HashSet<_> = stems.iter().collect();
            assert_eq!(unique.len(), stems.len(), "duplicate entry in {label}");
        }
    }

    /// The exclusion table is checked before the developer table, so an
    /// overlap would silently disable a developer entry.
    #[test]
    fn no_program_is_both_registered_and_excluded() {
        for p in DEDICATED.iter().chain(RUNTIMES.iter()) {
            assert!(
                excluded(p.stem).is_none(),
                "{} is in both the developer and exclusion tables",
                p.stem
            );
        }
    }

    /// Entries are compared against a lowercase stem with no extension. An
    /// entry written any other way can never match anything.
    #[test]
    fn every_entry_is_a_lowercase_stem_without_an_extension() {
        let stems = DEDICATED
            .iter()
            .chain(RUNTIMES.iter())
            .map(|p| p.stem)
            .chain(EXCLUDED.iter().map(|e| e.stem))
            .chain(SIGNATURES.iter().map(|s| s.token));
        for stem in stems {
            assert_eq!(stem, stem.to_ascii_lowercase(), "{stem} is not lowercase");
            assert!(!stem.ends_with(".exe"), "{stem} must not carry .exe");
            assert!(!stem.is_empty());
        }
    }

    /// The registry must not smuggle a port table in as a name. A numeric
    /// entry would be exactly the "known port means developer" rule the
    /// design forbids.
    #[test]
    fn no_entry_anywhere_is_a_port_number() {
        let all = DEDICATED
            .iter()
            .chain(RUNTIMES.iter())
            .map(|p| p.stem)
            .chain(EXCLUDED.iter().map(|e| e.stem))
            .chain(SIGNATURES.iter().map(|s| s.token));
        for entry in all {
            assert!(
                entry.parse::<u32>().is_err(),
                "{entry} is a bare number; the registry never keys on ports"
            );
        }
    }

    /// A runtime that could classify on its name alone would defeat the whole
    /// point of the two-table split.
    #[test]
    fn runtimes_are_never_marked_dedicated() {
        assert!(RUNTIMES.iter().all(|p| p.evidence == Evidence::Runtime));
        assert!(DEDICATED.iter().all(|p| p.evidence == Evidence::Dedicated));
    }

    /// Found by running a real demo environment: the Microsoft Store build of
    /// Python installs as `python3.12.exe`, and that is the name the process
    /// list shows. An exact-match-only registry called one of the most common
    /// Windows development runtimes unrecognised.
    #[test]
    fn a_versioned_runtime_name_still_resolves() {
        for name in ["python3.12", "python3.13", "python3", "python2", "python"] {
            let p = program(name).unwrap_or_else(|| panic!("{name} should resolve"));
            assert_eq!(p.display, "Python", "{name}");
            assert_eq!(p.evidence, Evidence::Runtime);
        }
    }

    /// The retry runs only after an exact match fails, so a program whose real
    /// name ends in digits keeps its own entry.
    #[test]
    fn an_exact_match_always_wins_over_the_version_retry() {
        assert_eq!(
            excluded("msedgewebview2").unwrap().display,
            "the Edge WebView2 runtime"
        );
        assert_eq!(excluded("obs64").unwrap().display, "OBS Studio");
        assert_eq!(
            excluded("nvsphelper64").unwrap().display,
            "an NVIDIA helper"
        );
    }

    #[test]
    fn the_version_retry_cannot_invent_a_match() {
        // Nothing but digits, and nothing that resolves after trimming.
        assert!(program("12345").is_none());
        assert!(program("3.12").is_none());
        assert!(program("notaprogram9").is_none());
        assert!(excluded("87654").is_none());
        // And it never turns a non-match into a prefix match.
        assert!(program("nodejs-updater").is_none());
        assert!(program("mongodb-compass").is_none());
    }

    #[test]
    fn unversioned_trims_only_a_trailing_version() {
        assert_eq!(unversioned("python3.12"), Some("python"));
        assert_eq!(unversioned("node20"), Some("node"));
        assert_eq!(unversioned("python"), None, "nothing to trim");
        assert_eq!(unversioned("123"), None, "no letters left");
        assert_eq!(unversioned(""), None);
    }

    #[test]
    fn lookups_are_exact_and_case_insensitive() {
        assert!(program("mongod").is_some());
        assert!(program("MongoD").is_some());
        assert!(program("node").is_some());
        // Not a prefix, not a substring.
        assert!(program("node-updater").is_none());
        assert!(program("mongodb-compass").is_none());
        assert!(program("nod").is_none());

        assert!(excluded("chrome").is_some());
        assert!(excluded("CHROME").is_some());
        assert!(excluded("chrome-remote").is_none());

        assert!(signature("vite").is_some());
        assert!(signature("VITE").is_some());
        assert!(signature("vitest").is_some());
        assert!(signature("invite").is_none());
    }

    /// Editors are deliberately unregistered. If one is ever added to either
    /// table it must be a decision, not a drift.
    #[test]
    fn editors_are_in_neither_table() {
        for editor in [
            "code",
            "cursor",
            "devenv",
            "idea64",
            "pycharm64",
            "webstorm64",
            "rider64",
            "sublime_text",
            "zed",
        ] {
            assert!(
                program(editor).is_none(),
                "{editor} must not be a developer program"
            );
            assert!(excluded(editor).is_none(), "{editor} must not be excluded");
        }
    }

    /// The version exists so a classification can be pinned to a specific set
    /// of tables. That is only true if it travels with them, so this asserts it
    /// is reachable and set rather than comparing a constant with a literal.
    #[test]
    fn the_registry_version_is_readable_and_set() {
        let version: u32 = REGISTRY_VERSION;
        assert_ne!(version, 0, "an unset registry version pins nothing");
    }
}
