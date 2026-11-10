import { expect } from 'bun:test';
import { test } from '@omochi-ui/core/testing';
import { createDashboardNavigation } from '../src/dashboard-navigation.js';
test('leave decisions come from the active editor, and stale cleanup cannot remove its successor', async () => {
  const navigation = createDashboardNavigation();
  expect(await navigation.requestLeave()).toBe(true);
  const old = navigation.register(async () => true);
  const current = navigation.register(async () => false);
  old();
  expect(await navigation.requestLeave()).toBe(false);
  current();
  expect(await navigation.requestLeave()).toBe(true);
});
test('confirmation settles before a caller can replace its session', async () => {
  const navigation = createDashboardNavigation();
  let answer!: (allowed: boolean) => void;
  navigation.register(
    () =>
      new Promise((resolve) => {
        answer = resolve;
      }),
  );
  let replaced = false;
  const operation = navigation.requestLeave().then((allowed) => {
    replaced = allowed;
  });
  await Promise.resolve();
  expect(replaced).toBe(false);
  answer(true);
  await operation;
  expect(replaced).toBe(true);
});
