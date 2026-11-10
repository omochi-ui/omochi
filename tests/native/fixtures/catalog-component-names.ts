import type {
  Card2Props,
  SelfProps,
  _BadgeProps,
  classProps,
  GeneratedCatalogComponentMap,
} from './catalog.js';

const card: Card2Props = { label: 'card' };
const self: SelfProps = { label: 'self' };
const badge: _BadgeProps = { label: 'badge' };
const keyword: classProps = { label: 'keyword' };
const components: GeneratedCatalogComponentMap = {
  Card2: card,
  Self: self,
  _Badge: badge,
  class: keyword,
};
// @ts-expect-error Component wire names retain their exact case and underscores.
const renamed: keyof GeneratedCatalogComponentMap = 'Badge';
console.log(JSON.stringify(components));
void renamed;
