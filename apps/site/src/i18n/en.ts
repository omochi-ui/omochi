import type { Messages } from './ja';

export const en = {
  meta: {
    title: 'omochi — Attach a UI to your program',
    description:
      'Build browser UIs in Python, TypeScript, or Rust. Keep your state and actions in your program. Bring your tools together in one dashboard.',
    ogDescription: 'Your language. Your logic. A little UI.',
  },
  nav: {
    skip: 'Skip to content',
    home: 'omochi home',
    label: 'Main navigation',
    docs: 'Docs',
    github: 'GitHub',
    language: '日本語',
  },
  landing: {
    hero: {
      title: 'Your program.',
      emphasis: 'With a little UI.',
      description:
        'Turn the code you already have into a tool you can see, touch, and share. State and actions stay in your language. Omochi brings them to the browser.',
      cta: 'Build your first UI',
      more: 'See how it works',
      note: 'Local tools. Browser views. Apache 2.0.',
    },
    workbench: {
      label: 'Code and dashboard preview',
      preview: 'A view in your browser',
      example: 'EXAMPLE VIEW',
      description: 'Run Python code with a button.',
      connection: 'Program → A2UI → Browser',
      eyebrow: 'FROM A PROGRAM TO A WORKSPACE',
      counter: 'Count: 0',
      button: 'Add one',
    },
    how: {
      title: 'Keep the logic close.',
      description:
        'Define a view where your application already lives. The shared renderer takes care of showing it.',
      declareTitle: 'Declare a view',
      declareDescription:
        'A view is called a Surface in the SDK. Describe text, inputs, and controls in Python, TypeScript, or Rust.',
      actionTitle: 'Connect an action',
      actionDescription:
        'Use your existing functions. Publish state updates, including progress during a long task.',
      renderTitle: 'Let the browser render',
      renderDescription:
        'Omochi sends A2UI updates. Unfinished form input stays in the browser.',
      eyebrow: '01 / A SMALLER DISTANCE',
    },
    dashboard: {
      title: 'One script is a start.',
      continuation: 'A workspace is next.',
      description:
        'Bring views from running applications into a dashboard. Add a command button, follow a log, or give an agent a place to show its work.',
      alt: 'Omochi dashboard showing application views and dashboard navigation',
      caption: 'The Omochi dashboard · actual application screenshot',
      eyebrow: '02 / A PLACE FOR YOUR TOOLS',
    },
    paths: {
      sdkTitle: 'A UI for your code',
      sdkDescription: 'State, actions, and views in three languages.',
      utilsTitle: 'A handle for your tools',
      utilsDescription: 'Buttons, processes, logs, and health checks.',
      mcpTitle: 'A view for your agent',
      mcpDescription: 'Let agents publish views and request approval.',
      eyebrow: '03 / FIND YOUR STARTING POINT',
    },
    closing: {
      title: 'A little UI goes a long way.',
      cta: 'Open the docs',
      eyebrow: 'MAKE SOMETHING USEFUL',
    },
  },
  footer: {
    description: 'Attach a UI to your program.',
    license: 'Apache 2.0',
  },
} satisfies Messages;
