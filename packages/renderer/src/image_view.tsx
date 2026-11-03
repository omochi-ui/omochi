import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { type BrowserImageScope, type BrowserImage } from './images.js';

/** Decode one raster frame; retain no URL cache or animated decoder. */
export function ScopedImage({ scope, uri, description, style }: { scope?: BrowserImageScope; uri: string; description: string; style: CSSProperties }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<{ scope?: BrowserImageScope; uri: string; ready: boolean; error: boolean }>();
  useEffect(() => {
    const cancel = new AbortController();
    const target = canvas.current;
    let retained: BrowserImage | undefined;
    const revoke = () => {
      cancel.abort(); retained?.dispose(); retained = undefined;
      if (target) { target.width = 0; target.height = 0; }
    };
    scope?.signal.addEventListener('abort', revoke, { once: true });
    setState({ scope, uri, ready: false, error: false });
    const load = async () => {
      try {
        if (!scope) throw new Error('Image source unavailable');
        retained = await scope.load(uri, cancel.signal);
        if (cancel.signal.aborted || !target) { retained.dispose(); retained = undefined; return; }
        const drawing = target.getContext('2d');
        if (!drawing) throw new Error('Image display unavailable');
        target.width = retained.bitmap.width; target.height = retained.bitmap.height;
        drawing.drawImage(retained.bitmap, 0, 0);
        retained.bitmap.close();
        setState({ scope, uri, ready: true, error: false });
      } catch {
        retained?.dispose(); retained = undefined;
        if (target) { target.width = 0; target.height = 0; }
        if (!cancel.signal.aborted) setState({ scope, uri, ready: false, error: true });
      }
    };
    void load();
    return () => {
      scope?.signal.removeEventListener('abort', revoke);
      revoke();
    };
  }, [scope, uri, attempt]);
  const current = state?.scope === scope && state?.uri === uri ? state : undefined;
  return <>
    <canvas ref={canvas} role="img" aria-label={description || 'Image'} style={{ ...style, display: current?.ready ? 'block' : 'none' }} />
    {!current?.ready && <span><span role="img" aria-label={description || 'Image'}>{current?.error ? 'Image unavailable' : 'Loading image…'}</span>{current?.error && <button type="button" onClick={() => setAttempt(value => value + 1)}>Retry image</button>}</span>}
  </>;
}
