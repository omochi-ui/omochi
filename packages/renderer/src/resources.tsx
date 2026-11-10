import { mergeObjectsFunction, nullInputFunction } from './expressions.js';
import {
  createContext,
  createElement,
  useContext,
  type CSSProperties,
} from 'react';
import { splitRouteId, type ImagePolicy } from '@omochi-ui/core/browser';
import { ScopedImage } from './image_view.js';
import { ScopedMarkdown } from './markdown.js';
import { type BrowserImageScope } from './images.js';
import { Catalog, type ComponentContext } from '@a2ui/web_core/v0_9';
import type { WebComponentImplementation } from '@a2ui/web_core/v0_9/universal';
import {
  A2uiColumn,
  A2uiRow,
  A2uiText as TextApi,
  A2uiImage as ImageApi,
  A2uiVideo as VideoApi,
  A2uiAudioPlayer as AudioPlayerApi,
  basicCatalog,
} from '@a2ui/web_core/v0_9/basic_catalog';
import {
  createComponentImplementation,
  type ReactCatalogComponent,
} from '@a2ui/react/v0_9';

/** Host an upstream basic element while retaining React resource-policy wrappers. */
function BasicView({
  implementation,
  context,
}: {
  implementation: WebComponentImplementation;
  context: ComponentContext;
}) {
  if (
    typeof customElements !== 'undefined' &&
    !customElements.get(implementation.tagName)
  ) {
    customElements.define(implementation.tagName, implementation.element);
  }
  return createElement(implementation.tagName, {
    ref: (element: (HTMLElement & { context: ComponentContext }) | null) => {
      if (element) element.context = context;
    },
  });
}

export const ResourceContext = createContext({
  endpoint: '',
  surfaceId: '',
  baseUrl: undefined as string | undefined,
  scopedImages: false,
  imageScope: undefined as BrowserImageScope | undefined,
});
export function resolveResourceUrl(
  value: string,
  endpoint: string,
  surfaceId: string,
  baseUrl?: string,
): string {
  if (typeof value !== 'string') return '';
  if (!value.startsWith('omochi:')) return value;
  const id = /^omochi:\/\/resources\/([a-f0-9]{32})$/.exec(value)?.[1];
  if (!id) return '';
  const url = new URL(
    baseUrl ?? endpoint,
    globalThis.location?.href ?? 'http://localhost',
  );
  url.protocol =
    url.protocol === 'wss:'
      ? 'https:'
      : url.protocol === 'ws:'
        ? 'http:'
        : url.protocol;
  const channel = surfaceId.split('/')[0]!;
  const route = splitRouteId(channel) ? channel : '_';
  url.pathname = `/resources/${encodeURIComponent(route)}/${id}`;
  url.search = '';
  url.hash = '';
  return url.href;
}
function useResource(value: string) {
  const context = useContext(ResourceContext);
  return resolveResourceUrl(
    value,
    context.endpoint,
    context.surfaceId,
    context.baseUrl,
  );
}
const Text = createComponentImplementation(TextApi, ({ props, context }) => {
  const { scopedImages, imageScope } = useContext(ResourceContext);
  if (
    !scopedImages ||
    (props.variant &&
      ['h1', 'h2', 'h3', 'h4', 'h5', 'caption'].includes(props.variant))
  ) {
    return <BasicView implementation={TextApi} context={context} />;
  }
  return (
    <div
      className={props.variant ?? 'body'}
      style={{
        minWidth: 0,
        flexGrow: props.weight,
        flexBasis: props.weight !== undefined ? 0 : undefined,
      }}
    >
      <ScopedMarkdown
        imageScope={imageScope}
        text={
          typeof props.text === 'string' ? props.text : String(props.text ?? '')
        }
      />
    </div>
  );
});
function imageStyle(props: {
  variant?: string;
  fit?: CSSProperties['objectFit'] | 'scaleDown';
  weight?: number;
}): CSSProperties {
  return {
    display: 'block',
    maxWidth:
      props.variant === 'smallFeature'
        ? 'var(--a2ui-image-small-feature-size, 100px)'
        : '100%',
    objectFit: props.fit === 'scaleDown' ? 'scale-down' : (props.fit ?? 'fill'),
    borderRadius:
      props.variant === 'avatar' ? '50%' : 'var(--a2ui-image-border-radius, 0)',
    width:
      props.variant === 'icon'
        ? 'var(--a2ui-image-icon-size, 24px)'
        : props.variant === 'avatar'
          ? 'var(--a2ui-image-avatar-size, 40px)'
          : undefined,
    flexGrow: props.weight,
    flexBasis: props.weight !== undefined ? 0 : undefined,
    maxHeight:
      props.variant === 'largeFeature'
        ? 'var(--a2ui-image-large-feature-size, 400px)'
        : undefined,
    height:
      props.variant === 'icon'
        ? 24
        : props.variant === 'avatar'
          ? 40
          : props.variant === 'header'
            ? 200
            : undefined,
  };
}
const Image = createComponentImplementation(ImageApi, ({ props, context }) => {
  const src = useResource(props.url);
  const images = useContext(ResourceContext);
  const style = imageStyle(props);
  if (images.scopedImages)
    return (
      <ScopedImage
        scope={images.imageScope}
        uri={props.url}
        description={props.description ?? ''}
        style={style}
      />
    );
  if (!(typeof props.url === 'string' && props.url.startsWith('omochi:')))
    return <BasicView implementation={ImageApi} context={context} />;
  return (
    <img src={src || undefined} alt={props.description ?? ''} style={style} />
  );
});
const Video = createComponentImplementation(VideoApi, ({ props, context }) => {
  const src = useResource(props.url);
  return typeof props.url === 'string' && props.url.startsWith('omochi:') ? (
    <video src={src || undefined} controls style={{ maxWidth: '100%' }} />
  ) : (
    <BasicView implementation={VideoApi} context={context} />
  );
});
const AudioPlayer = createComponentImplementation(
  AudioPlayerApi,
  ({ props, context }) => {
    const src = useResource(props.url);
    return typeof props.url === 'string' && props.url.startsWith('omochi:') ? (
      <div>
        <audio src={src || undefined} controls />
        {props.description && <p>{props.description}</p>}
      </div>
    ) : (
      <BasicView implementation={AudioPlayerApi} context={context} />
    );
  },
);
// The upstream layout elements access a disposed controller in updated().
// Keep layout in React so a queued update after surface removal is harmless.
function layoutStyle(
  props: { justify?: string; align?: string; weight?: number },
  direction: 'row' | 'column',
): CSSProperties {
  const justify: Record<string, CSSProperties['justifyContent']> = {
    start: 'flex-start',
    end: 'flex-end',
    center: 'center',
    spaceBetween: 'space-between',
    spaceAround: 'space-around',
    spaceEvenly: 'space-evenly',
    stretch: 'stretch',
  };
  const align: Record<string, CSSProperties['alignItems']> = {
    start: 'flex-start',
    end: 'flex-end',
    center: 'center',
    stretch: 'stretch',
    baseline: 'baseline',
  };
  return {
    display: 'flex',
    flexDirection: direction,
    width: '100%',
    minWidth: 0,
    gap: `var(--a2ui-${direction}-gap, var(--a2ui-spacing-m))`,
    justifyContent: justify[props.justify ?? 'start'],
    alignItems: align[props.align ?? 'stretch'],
    flexGrow: props.weight,
    flexBasis: props.weight !== undefined ? 0 : undefined,
  };
}
const Column = createComponentImplementation(
  A2uiColumn,
  ({ props, buildChild }) => (
    <div
      role="group"
      aria-label={props.accessibility?.label}
      aria-description={props.accessibility?.description}
      style={layoutStyle(props, 'column')}
    >
      {Array.isArray(props.children) &&
        props.children.map((child) =>
          typeof child === 'string'
            ? buildChild(child)
            : buildChild(child.id, child.basePath),
        )}
    </div>
  ),
);
const Row = createComponentImplementation(A2uiRow, ({ props, buildChild }) => (
  <div
    role="group"
    aria-label={props.accessibility?.label}
    aria-description={props.accessibility?.description}
    style={layoutStyle(props, 'row')}
  >
    {Array.isArray(props.children) &&
      props.children.map((child) =>
        typeof child === 'string'
          ? buildChild(child)
          : buildChild(child.id, child.basePath),
      )}
  </div>
));
const layoutReplacements = [
  ['Column', Column],
  ['Row', Row],
] as const;
const replacements = new Map([
  ...layoutReplacements,
  ['Text', Text],
  ['Image', Image],
  ['Video', Video],
  ['AudioPlayer', AudioPlayer],
]);
const DirectText = createComponentImplementation(TextApi, ({ props }) => {
  const { imageScope } = useContext(ResourceContext);
  const tag =
    props.variant && ['h1', 'h2', 'h3', 'h4', 'h5'].includes(props.variant)
      ? props.variant
      : 'div';
  return createElement(
    tag,
    {
      className: props.variant ?? 'body',
      style: { minWidth: 0, flexGrow: props.weight },
    },
    <ScopedMarkdown
      imageScope={imageScope}
      text={
        typeof props.text === 'string' ? props.text : String(props.text ?? '')
      }
    />,
  );
});
const DirectImage = createComponentImplementation(ImageApi, ({ props }) => {
  const { imageScope } = useContext(ResourceContext);
  return (
    <ScopedImage
      scope={imageScope}
      uri={props.url}
      description={props.description ?? ''}
      style={imageStyle(props)}
    />
  );
});
const DirectVideo = createComponentImplementation(VideoApi, () => (
  <span role="status">Video unavailable in direct mode</span>
));
const DirectAudio = createComponentImplementation(AudioPlayerApi, () => (
  <span role="status">Audio unavailable in direct mode</span>
));
const directReplacements = new Map([
  ...layoutReplacements,
  ['Text', DirectText],
  ['Image', DirectImage],
  ['Video', DirectVideo],
  ['AudioPlayer', DirectAudio],
]);

const HubVideo = createComponentImplementation(VideoApi, () => (
  <span role="status">Video unavailable for shared dashboards</span>
));
const HubAudio = createComponentImplementation(AudioPlayerApi, () => (
  <span role="status">Audio unavailable for shared dashboards</span>
));
const hubReplacements = new Map([
  ...directReplacements,
  ['Video', HubVideo],
  ['AudioPlayer', HubAudio],
]);
export function createResourceCatalog(policy: ImagePolicy) {
  const selected =
    policy === 'native'
      ? replacements
      : policy === 'hub'
        ? hubReplacements
        : directReplacements;
  return new Catalog<ReactCatalogComponent>(
    basicCatalog.id,
    basicCatalog.protocolVersion,
    [...basicCatalog.components].map(
      ([name, component]) => selected.get(name) ?? component,
    ),
    [
      ...basicCatalog.functions.values(),
      mergeObjectsFunction,
      nullInputFunction,
    ],
    basicCatalog.themeSchema,
  );
}
export const resourceCatalog = createResourceCatalog('native');
