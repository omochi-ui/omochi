import assert from 'node:assert/strict';
import { test } from '@omochi-ui/core/testing';
import { renderToStaticMarkup } from 'react-dom/server';
import { ScopedMarkdown } from '../src/markdown.js';
import { ScopedImage } from '../src/image_view.js';
import { BrowserImages } from '../src/images.js';

test('scoped Text Markdown and Image never put service URLs into browser image elements', () => {
  const images = new BrowserImages('ws://localhost/ws', () => {});
  images.update('main', 'opaque');
  const html = renderToStaticMarkup(<>
    <ScopedMarkdown imageScope={images.scope('main')} text={'**Before** ![Diagram](https://private.example/markdown.png?secret=hidden) and [explicit link](https://example.com/)\n\n- Item'} />
    <ScopedImage scope={images.scope('main')} uri="https://private.example/image.png?secret=hidden" description="Photo" style={{}} />
  </>);
  assert.equal((html.match(/<canvas/g) ?? []).length, 2);
  assert(!html.includes('<img'));
  assert(!html.includes('private.example'));
  assert(!html.includes('hidden'));
  assert(html.includes('<strong>Before</strong>'));
  assert(html.includes('href="https://example.com/"'));
  assert(html.includes('<li>'));
  images.close();
});
