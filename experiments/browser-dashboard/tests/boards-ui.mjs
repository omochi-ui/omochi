import { writeFile } from 'node:fs/promises';
import path from 'node:path';

export function fixtureBoard(fixture) {
  return {
    format: 'omochi.board',
    version: 1,
    board: {
      name: 'Fixture applications',
      panels: fixture.ready.gateways.flatMap((gateway, node) =>
        [1, 16, 17, 64].map((length, index) => ({
          id: `gateway-${node}-service-${index}`,
          title: `Fixture ${node} service ${index}`,
          reference: {
            nodeId: gateway.nodeId,
            serviceId: 's'.repeat(length),
            surfaceId: 'main/nested',
          },
          x: node * 6,
          y: index * 8,
          w: 6,
          h: 8,
        })),
      ),
    },
    sources: fixture.ready.gateways
      .map((gateway) => ({ nodeId: gateway.nodeId, name: gateway.name }))
      .sort((a, b) => a.nodeId.localeCompare(b.nodeId)),
  };
}
/** The same visible import-as-copy workflow used by recipients; no private repository/session globals. */
export async function chooseBoardFile(
  browser,
  directory,
  document,
  until,
  name = 'fixture-board.json',
) {
  const file = path.join(directory, name);
  await writeFile(file, JSON.stringify(document));
  await until(
    async () =>
      await browser.evaluate('!!document.querySelector("#import-board-file")'),
    'shared board import control',
  );
  const root = await browser.command('DOM.getDocument');
  const input = await browser.command('DOM.querySelector', {
    nodeId: root.root.nodeId,
    selector: '#import-board-file',
  });
  await browser.command('DOM.setFileInputFiles', {
    nodeId: input.nodeId,
    files: [file],
  });
}
export async function previewBoardFile(
  browser,
  directory,
  document,
  until,
  name = 'fixture-board.json',
) {
  await chooseBoardFile(browser, directory, document, until, name);
  await until(
    async () =>
      await browser.evaluate(
        '!!document.querySelector("dialog[open] #import-title")',
      ),
    'validated import preview',
  );
}
export async function importBoardFile(
  browser,
  directory,
  document,
  until,
  name = 'fixture-board.json',
) {
  await previewBoardFile(browser, directory, document, until, name);
  const previous = await browser.evaluate(
    'new URLSearchParams(location.search).get("dashboard")',
  );
  await browser.evaluate(
    'Array.from(document.querySelectorAll("dialog[open] button")).find(button=>button.textContent==="Import as new dashboard").click()',
  );
  await until(
    async () =>
      await browser.evaluate(
        `!document.querySelector('dialog[open] #import-title')&&document.querySelector('.announcement')?.textContent.includes('Board imported')&&new URLSearchParams(location.search).get('dashboard')!==${JSON.stringify(previous)}`,
      ),
    'import-as-copy commit',
  );
}
