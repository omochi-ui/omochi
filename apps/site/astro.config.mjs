import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import sitemap from '@astrojs/sitemap';
export default defineConfig({
  site: 'https://omochi.fyi',
  trailingSlash: 'always',
  i18n: {
    defaultLocale: 'ja',
    locales: ['ja', 'en'],
    routing: { prefixDefaultLocale: false },
  },
  integrations: [
    sitemap(),
    starlight({
      favicon: '/assets/mark.svg',
      disable404Route: true,
      // Starlight 0.42.8 normalizes string titles before applying Astro's default locale.
      title: { ja: 'omochi', en: 'omochi' },
      description: 'Attach a UI to your program.',
      // The landing page's Astro Code component also needs the github-dark Shiki theme.
      expressiveCode: { removeUnusedThemes: false },
      logo: { src: './public/assets/mark.svg', replacesTitle: false },
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/omochi-ui/omochi',
        },
      ],
      customCss: ['./src/styles/docs.css'],
      editLink: {
        baseUrl: 'https://github.com/omochi-ui/omochi/edit/main/apps/site/',
      },
      sidebar: [
        {
          label: 'はじめる',
          translations: { en: 'Start here' },
          items: [{ autogenerate: { directory: 'docs/start' } }],
        },
        {
          label: '基本概念',
          translations: { en: 'Concepts' },
          items: [{ autogenerate: { directory: 'docs/concepts' } }],
        },
        {
          label: '実践ガイド',
          translations: { en: 'Guides' },
          items: [{ autogenerate: { directory: 'docs/guides' } }],
        },
        {
          label: 'リファレンス',
          translations: { en: 'Reference' },
          items: [{ autogenerate: { directory: 'docs/reference' } }],
        },
        {
          label: '仕組みと問題解決',
          translations: { en: 'Understand & troubleshoot' },
          items: [{ autogenerate: { directory: 'docs/understand' } }],
        },
      ],
      head: [
        {
          tag: 'meta',
          attrs: {
            property: 'og:image',
            content: 'https://omochi.fyi/assets/og.png',
          },
        },
      ],
    }),
  ],
});
