import { expect } from 'bun:test';
import { test } from '@omochi-ui/core/testing';
import { createFragmentInbox } from '../src/remote/fragment.js';

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
