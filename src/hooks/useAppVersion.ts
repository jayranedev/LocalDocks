import { useEffect, useState } from 'react';
import { getAppVersion } from '../lib/ipc';

/**
 * The application version, or `null` until it is known.
 *
 * Asked once per mount and rendered as absent rather than guessed if the app
 * cannot answer — in a browser-only dev session there is no packaged app to
 * ask, and showing a made-up number there would be the same defect this hook
 * exists to fix.
 */
export function useAppVersion(): string | null {
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    getAppVersion().then((v) => {
      if (alive) setVersion(v);
    });
    return () => {
      alive = false;
    };
  }, []);

  return version;
}
