import MarkdownIt from 'markdown-it';
import { createElement, useMemo, type ReactNode } from 'react';
import { type BrowserImageScope } from './images.js';
import { ScopedImage } from './image_view.js';

const markdown = new MarkdownIt({ html: false });
/** Render Markdown tokens as elements: image URLs are never inserted into HTML. */
export function ScopedMarkdown({ text, imageScope }: { text: string; imageScope?: BrowserImageScope }) {
  return useMemo(() => {
    const render = (tokens: ReturnType<MarkdownIt['parse']>): ReactNode[] => {
      let cursor = 0;
      const children = (): ReactNode[] => {
        const result: ReactNode[] = [];
        while (cursor < tokens.length) {
          const key = cursor++;
          const token = tokens[key]!;
          if (token.nesting === -1) break;
          if (token.type === 'inline') { result.push(...render(token.children ?? [])); continue; }
          if (token.type === 'text') { result.push(token.content); continue; }
          if (token.type === 'image') {
            result.push(<ScopedImage key={key} scope={imageScope} uri={token.attrGet('src') ?? ''} description={token.content} style={{ maxWidth: '100%' }} />);
          } else if (token.type === 'code_inline') result.push(<code key={key}>{token.content}</code>);
          else if (token.type === 'fence' || token.type === 'code_block') result.push(<pre key={key}><code>{token.content}</code></pre>);
          else if (token.type === 'softbreak') result.push('\n');
          else if (token.type === 'hardbreak') result.push(<br key={key} />);
          else if (token.type === 'hr') result.push(<hr key={key} />);
          else if (token.nesting === 1) {
            const props: Record<string, unknown> = { key };
            if (token.tag === 'a') { props.href = token.attrGet('href'); props.title = token.attrGet('title'); props.rel = 'noopener noreferrer'; }
            if (token.tag === 'ol' && token.attrGet('start')) props.start = Number(token.attrGet('start'));
            const nested = children();
            result.push(token.hidden ? <span key={key}>{nested}</span> : createElement(token.tag, props, nested));
          } else result.push(token.content);
        }
        return result;
      };
      return children();
    };
    return render(markdown.parse(text, {}));
  }, [text, imageScope]);
}
