import { createContext, useContext } from 'react';
import { Catalog } from '@a2ui/web_core/v0_9';
import { ImageApi, VideoApi, AudioPlayerApi } from '@a2ui/web_core/v0_9/basic_catalog';
import { Image as BasicImage, Video as BasicVideo, AudioPlayer as BasicAudioPlayer, basicCatalog, createComponentImplementation } from '@a2ui/react/v0_9';

export const ResourceContext = createContext({ endpoint: '', surfaceId: '', baseUrl: undefined as string | undefined });
export function resolveResourceUrl(value: string, endpoint: string, surfaceId: string, baseUrl?: string): string {
  if (typeof value !== 'string') return '';
  if (!value.startsWith('omochi:')) return value;
  const id = /^omochi:\/\/resources\/([a-f0-9]{32})$/.exec(value)?.[1];
  if (!id) return '';
  const url = new URL(baseUrl ?? endpoint, globalThis.location?.href ?? 'http://localhost');
  url.protocol = url.protocol === 'wss:' ? 'https:' : url.protocol === 'ws:' ? 'http:' : url.protocol;
  const channel = surfaceId.split('/')[0]!;
  const route = /^[a-z2-7]{16}~[A-Za-z0-9._-]{1,64}$/.test(channel) ? channel : '_';
  url.pathname = `/resources/${encodeURIComponent(route)}/${id}`;
  url.search = ''; url.hash = '';
  return url.href;
}
function useResource(value: string) {
  const context = useContext(ResourceContext);
  return resolveResourceUrl(value, context.endpoint, context.surfaceId, context.baseUrl);
}
const Image = createComponentImplementation(ImageApi, ({ props, context, buildChild }) => {
  const src = useResource(props.url);
  if (!(typeof props.url === 'string' && props.url.startsWith('omochi:'))) return <BasicImage.render context={context} buildChild={buildChild} />;
  const style = { display: 'block', maxWidth: props.variant === 'smallFeature' ? 'var(--a2ui-image-small-feature-size, 100px)' : '100%', objectFit: props.fit === 'scaleDown' ? 'scale-down' as const : props.fit ?? 'fill',
    borderRadius: props.variant === 'avatar' ? '50%' : 'var(--a2ui-image-border-radius, 0)',
    width: props.variant === 'icon' ? 'var(--a2ui-image-icon-size, 24px)' : props.variant === 'avatar' ? 'var(--a2ui-image-avatar-size, 40px)' : undefined,
    flexGrow: props.weight, flexBasis: props.weight !== undefined ? 0 : undefined,
    maxHeight: props.variant === 'largeFeature' ? 'var(--a2ui-image-large-feature-size, 400px)' : undefined,
    height: props.variant === 'icon' ? 24 : props.variant === 'avatar' ? 40 : props.variant === 'header' ? 200 : undefined };
  return <img src={src || undefined} alt={props.description ?? ''} style={style} />;
});
const Video = createComponentImplementation(VideoApi, ({ props, context, buildChild }) => { const src = useResource(props.url); return (typeof props.url === 'string' && props.url.startsWith('omochi:')) ? <video src={src || undefined} controls style={{ maxWidth: '100%' }} /> : <BasicVideo.render context={context} buildChild={buildChild} />; });
const AudioPlayer = createComponentImplementation(AudioPlayerApi, ({ props, context, buildChild }) => { const src = useResource(props.url); return (typeof props.url === 'string' && props.url.startsWith('omochi:')) ? <div><audio src={src || undefined} controls />{props.description && <p>{props.description}</p>}</div> : <BasicAudioPlayer.render context={context} buildChild={buildChild} />; });
const replacements = new Map([['Image', Image], ['Video', Video], ['AudioPlayer', AudioPlayer]]);
export const resourceCatalog = new Catalog(basicCatalog.id,
  [...basicCatalog.components].map(([name, component]) => replacements.get(name) ?? component),
  [...basicCatalog.functions.values()], basicCatalog.themeSchema);
