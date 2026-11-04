import { expect } from 'bun:test';
import { test } from '@omochi-ui/core/testing';
import { createFragmentInbox, createHostedFragmentInbox } from '../src/remote/fragment.js';

test('navigation during module loading delivers only the latest link once', () => {
  const inbox = createFragmentInbox();
  inbox.receive('#share:first');
  inbox.receive('#owner:latest');
  const received: string[] = [];
  const unsubscribe = inbox.subscribe(fragment => received.push(fragment));
  expect(received).toEqual(['#owner:latest']);
  inbox.receive('#share:ready');
  expect(received).toEqual(['#owner:latest', '#share:ready']);
  unsubscribe();
  inbox.subscribe(fragment => received.push(fragment));
  expect(received).toHaveLength(2);
});

test('stale cleanup cannot remove a replacement receiver or lose later navigation', () => {
  const inbox = createFragmentInbox();
  const stale = inbox.subscribe(() => { throw new Error('stale receiver'); });
  const received: string[] = [];
  const current = inbox.subscribe(fragment => received.push(fragment));
  stale(); inbox.receive('#owner:active'); current();
  inbox.receive('#share:waiting');
  inbox.subscribe(fragment => received.push(fragment));
  expect(received).toEqual(['#owner:active', '#share:waiting']);
});

function hostedBrowser(hash = '') {
  let onHashChange = () => {};
  const browser = {
    location: { hash, pathname: '/hosted/', search: '?view=board' },
    history: {
      replaceState(_data: unknown, _unused: string, url: string | URL | null | undefined) {
        expect(url).toBe('/hosted/?view=board');
        browser.location.hash = '';
      },
    },
    addEventListener(_type: string, listener: () => void) { onHashChange = listener; },
  };
  return { browser: browser as unknown as Window, changeHash() { onHashChange(); } };
}

test('gateway navigation is scrubbed while bootstrap waits and supersedes the initial invitation', () => {
  const { browser, changeHash } = hostedBrowser('#g:initial');
  const fragments = createHostedFragmentInbox(browser);
  expect(browser.location.hash).toBe('');
  browser.location.hash = '#g:latest';
  changeHash();
  expect(browser.location.hash).toBe('');
  expect(fragments.take()).toBe('#g:latest');
  expect(fragments.take()).toBeNull();
});

test('bootstrap reads the current gateway hash before a queued hashchange and delivers it once', () => {
  const { browser, changeHash } = hostedBrowser();
  const fragments = createHostedFragmentInbox(browser);
  browser.location.hash = '#g:current';
  expect(fragments.take()).toBe('#g:current');
  expect(browser.location.hash).toBe('');
  const received: string[] = [];
  fragments.subscribe(fragment => received.push(fragment));
  changeHash();
  expect(received).toEqual([]);
  browser.location.hash = '#g:mounted';
  changeHash();
  expect(browser.location.hash).toBe('');
  expect(received).toEqual(['#g:mounted']);
});

test('share and owner routing remains available alongside live gateway navigation', () => {
  const { browser, changeHash } = hostedBrowser('#share:initial');
  const fragments = createHostedFragmentInbox(browser);
  browser.location.hash = '#owner:latest';
  changeHash();
  expect(fragments.take()).toBe('#owner:latest');
  const received: string[] = [];
  fragments.subscribe(fragment => received.push(fragment));
  for (const fragment of ['#share:ready', '#g:ready', '#owner:ready']) {
    browser.location.hash = fragment;
    changeHash();
    expect(browser.location.hash).toBe('');
  }
  expect(received).toEqual(['#share:ready', '#g:ready', '#owner:ready']);
});
