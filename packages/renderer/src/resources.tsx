import { createContext, createElement, useContext, type CSSProperties } from 'react';
import { splitRouteId, type ImagePolicy } from '@omochi-ui/core/browser';
import { ScopedImage } from './image_view.js';
import { ScopedMarkdown } from './markdown.js';
import { type BrowserImageScope } from './images.js';
import { Catalog } from '@a2ui/web_core/v0_9';
import { TextApi, ImageApi, VideoApi, AudioPlayerApi } from '@a2ui/web_core/v0_9/basic_catalog';
import { Text as BasicText, Image as BasicImage, Video as BasicVideo, AudioPlayer as BasicAudioPlayer, basicCatalog, createComponentImplementation } from '@a2ui/react/v0_9';

export const ResourceContext = createContext({ endpoint: '', surfaceId: '', baseUrl: undefined as string | undefined, scopedImages: false, imageScope: undefined as BrowserImageScope | undefined });
export function resolveResourceUrl(value: string, endpoint: string, surfaceId: string, baseUrl?: string): string {
  if (typeof value !== 'string') return '';
  if (!value.startsWith('omochi:')) return value;
  const id = /^omochi:\/\/resources\/([a-f0-9]{32})$/.exec(value)?.[1];
  if (!id) return '';
  const url = new URL(baseUrl ?? endpoint, globalThis.location?.href ?? 'http://localhost');
  url.protocol = url.protocol === 'wss:' ? 'https:' : url.protocol === 'ws:' ? 'http:' : url.protocol;
  const channel = surfaceId.split('/')[0]!;
  const route = splitRouteId(channel) ? channel : '_';
  url.pathname = `/resources/${encodeURIComponent(route)}/${id}`;
  url.search = ''; url.hash = '';
  return url.href;
}
function useResource(value: string) {
  const context = useContext(ResourceContext);
  return resolveResourceUrl(value, context.endpoint, context.surfaceId, context.baseUrl);
}
const Text = createComponentImplementation(TextApi, ({ props, context, buildChild }) => {
  const { scopedImages, imageScope } = useContext(ResourceContext);
  if (!scopedImages || (props.variant && ['h1', 'h2', 'h3', 'h4', 'h5', 'caption'].includes(props.variant))) {
    return <BasicText.render context={context} buildChild={buildChild} />;
  }
  return <div className={props.variant ?? 'body'} style={{ minWidth: 0, flexGrow: props.weight, flexBasis: props.weight !== undefined ? 0 : undefined }}>
    <ScopedMarkdown imageScope={imageScope} text={typeof props.text === 'string' ? props.text : String(props.text ?? '')} />
  </div>;
});
function imageStyle(props: { variant?: string; fit?: CSSProperties['objectFit'] | 'scaleDown'; weight?: number }): CSSProperties {
  return { display: 'block', maxWidth: props.variant === 'smallFeature' ? 'var(--a2ui-image-small-feature-size, 100px)' : '100%', objectFit: props.fit === 'scaleDown' ? 'scale-down' : props.fit ?? 'fill',
    borderRadius: props.variant === 'avatar' ? '50%' : 'var(--a2ui-image-border-radius, 0)',
    width: props.variant === 'icon' ? 'var(--a2ui-image-icon-size, 24px)' : props.variant === 'avatar' ? 'var(--a2ui-image-avatar-size, 40px)' : undefined,
    flexGrow: props.weight, flexBasis: props.weight !== undefined ? 0 : undefined,
    maxHeight: props.variant === 'largeFeature' ? 'var(--a2ui-image-large-feature-size, 400px)' : undefined,
    height: props.variant === 'icon' ? 24 : props.variant === 'avatar' ? 40 : props.variant === 'header' ? 200 : undefined };
}
const Image = createComponentImplementation(ImageApi, ({ props, context, buildChild }) => {
  const src = useResource(props.url);
  const images = useContext(ResourceContext);
  const style = imageStyle(props);
  if (images.scopedImages) return <ScopedImage scope={images.imageScope} uri={props.url} description={props.description ?? ''} style={style} />;
  if (!(typeof props.url === 'string' && props.url.startsWith('omochi:'))) return <BasicImage.render context={context} buildChild={buildChild} />;
  return <img src={src || undefined} alt={props.description ?? ''} style={style} />;
});
const Video = createComponentImplementation(VideoApi, ({ props, context, buildChild }) => { const src = useResource(props.url); return (typeof props.url === 'string' && props.url.startsWith('omochi:')) ? <video src={src || undefined} controls style={{ maxWidth: '100%' }} /> : <BasicVideo.render context={context} buildChild={buildChild} />; });
const AudioPlayer = createComponentImplementation(AudioPlayerApi, ({ props, context, buildChild }) => { const src = useResource(props.url); return (typeof props.url === 'string' && props.url.startsWith('omochi:')) ? <div><audio src={src || undefined} controls />{props.description && <p>{props.description}</p>}</div> : <BasicAudioPlayer.render context={context} buildChild={buildChild} />; });
const replacements = new Map([['Text', Text], ['Image', Image], ['Video', Video], ['AudioPlayer', AudioPlayer]]);
const DirectText = createComponentImplementation(TextApi, ({ props }) => {
  const { imageScope } = useContext(ResourceContext);
  const tag = props.variant && ['h1', 'h2', 'h3', 'h4', 'h5'].includes(props.variant) ? props.variant : 'div';
  return createElement(tag, { className: props.variant ?? 'body', style: { minWidth: 0, flexGrow: props.weight } },
    <ScopedMarkdown imageScope={imageScope} text={typeof props.text === 'string' ? props.text : String(props.text ?? '')} />);
});
const DirectImage = createComponentImplementation(ImageApi, ({ props }) => {
  const { imageScope } = useContext(ResourceContext);
  return <ScopedImage scope={imageScope} uri={props.url} description={props.description ?? ''} style={imageStyle(props)} />;
});
const DirectVideo = createComponentImplementation(VideoApi, () => <span role="status">Video unavailable in direct mode</span>);
const DirectAudio = createComponentImplementation(AudioPlayerApi, () => <span role="status">Audio unavailable in direct mode</span>);
const directReplacements = new Map([['Text', DirectText], ['Image', DirectImage], ['Video', DirectVideo], ['AudioPlayer', DirectAudio]]);

const HubVideo = createComponentImplementation(VideoApi, () => <span role="status">Video unavailable for shared dashboards</span>);
const HubAudio = createComponentImplementation(AudioPlayerApi, () => <span role="status">Audio unavailable for shared dashboards</span>);
const hubReplacements = new Map([...directReplacements, ['Video', HubVideo], ['AudioPlayer', HubAudio]]);
export function createResourceCatalog(policy: ImagePolicy) {
  const selected = policy === 'native' ? replacements : policy === 'hub' ? hubReplacements : directReplacements;
  return new Catalog(basicCatalog.id,
    [...basicCatalog.components].map(([name, component]) => selected.get(name) ?? component),
    [...basicCatalog.functions.values()], basicCatalog.themeSchema);
}
export const resourceCatalog = createResourceCatalog('native');
