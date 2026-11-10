import { lstatSync, readFileSync } from 'node:fs';
import { basename, extname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const languages = [
  'rust',
  'python',
  'shell',
  'web',
  'kotlin',
  'java',
  'dart',
  'swift',
  'c',
] as const;
type Language = (typeof languages)[number];
type Task = { language: Language; files: string[]; commands: string[][] };

const extensions: Record<Exclude<Language, 'rust'>, string[]> = {
  python: ['.py', '.pyi'],
  shell: ['.sh', '.bash', '.zsh', '.mksh', '.bats'],
  web: [
    '.js',
    '.jsx',
    '.mjs',
    '.cjs',
    '.ts',
    '.tsx',
    '.mts',
    '.cts',
    '.css',
    '.scss',
    '.less',
    '.html',
    '.htm',
    '.astro',
  ],
  kotlin: ['.kt', '.kts'],
  java: ['.java'],
  dart: ['.dart'],
  swift: ['.swift'],
  c: ['.c', '.h', '.cc', '.cpp', '.cxx', '.hh', '.hpp', '.hxx', '.m', '.mm'],
};

function capture(command: string[], cwd: string): string {
  const result = Bun.spawnSync(command, {
    cwd,
    stdout: 'pipe',
    stderr: 'pipe',
  });
  if (result.exitCode !== 0) {
    throw new Error(result.stderr.toString().trim() || `${command[0]} failed`);
  }
  return result.stdout.toString();
}

function sourceFiles(root: string): string[] {
  const ignoreFile = resolve(root, '.formatignore');
  const ignores = readFileSync(ignoreFile, 'utf8')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith('#'))
    .map((pattern) => new Bun.Glob(pattern));
  const paths = capture(
    ['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'],
    root,
  ).split('\0');
  return [...new Set(paths)]
    .filter((path) => {
      if (!path || ignores.some((pattern) => pattern.match(path))) return false;
      try {
        // Deleted files and symlinks must not be passed to in-place formatters.
        return lstatSync(resolve(root, path)).isFile();
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code === 'ENOENT') return false;
        throw error;
      }
    })
    .sort();
}

function toolCommand(
  language: Exclude<Language, 'rust'>,
  check: boolean,
): string[] {
  switch (language) {
    case 'python':
      return [
        'uv',
        'run',
        '--locked',
        'ruff',
        'format',
        ...(check ? ['--check'] : []),
      ];
    case 'shell':
      return ['shfmt', '-i', '2', check ? '-l' : '-w'];
    case 'web':
      return [
        process.execPath,
        fileURLToPath(import.meta.resolve('prettier/bin/prettier.cjs')),
        check ? '--check' : '--write',
      ];
    case 'kotlin': {
      const command = process.env.KTFMT_JAR
        ? ['java', '-jar', resolve(process.env.KTFMT_JAR)]
        : ['ktfmt'];
      return [
        ...command,
        '--kotlinlang-style',
        ...(check ? ['--dry-run', '--set-exit-if-changed'] : []),
      ];
    }
    case 'java': {
      const command = process.env.GOOGLE_JAVA_FORMAT_JAR
        ? ['java', '-jar', resolve(process.env.GOOGLE_JAVA_FORMAT_JAR)]
        : ['google-java-format'];
      return [
        ...command,
        ...(check ? ['--dry-run', '--set-exit-if-changed'] : ['--replace']),
      ];
    }
    case 'dart':
      return [
        'dart',
        'format',
        ...(check ? ['--output=none', '--set-exit-if-changed'] : []),
      ];
    case 'swift':
      return [
        ...(Bun.which('swift-format') ? ['swift-format'] : ['swift', 'format']),
        ...(check ? ['lint', '--strict'] : ['format', '--in-place']),
      ];
    case 'c':
      return ['clang-format', ...(check ? ['--dry-run', '--Werror'] : ['-i'])];
  }
}

async function main(): Promise<number> {
  const args = process.argv.slice(2);
  let check = false;
  let list = false;
  const only = new Set<Language>();
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--check') check = true;
    else if (arg === '--list') list = true;
    else if (arg === '--only') {
      const language = args[++i];
      if (!languages.includes(language as Language)) {
        throw new Error(`--only requires one of: ${languages.join(', ')}`);
      }
      only.add(language as Language);
    } else if (arg === '--help' || arg === '-h') {
      console.log(`Usage: bun run format [--check] [--list] [--only LANGUAGE]...
Languages: ${languages.join(', ')}
--list prints selected files without requiring or running formatters.`);
      return 0;
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }

  const root = capture(
    ['git', 'rev-parse', '--show-toplevel'],
    process.cwd(),
  ).trim();
  const files = sourceFiles(root);
  const tasks: Task[] = [];
  for (const language of languages) {
    if (only.size && !only.has(language)) continue;
    if (language === 'rust') {
      // Select each package explicitly: --all would also format vendored path
      // dependencies. This includes macros and packages in separate workspaces.
      const manifests = files.filter((path) => basename(path) === 'Cargo.toml');
      const commands: string[][] = [];
      const packages: string[] = [];
      for (const path of manifests) {
        const manifest = Bun.TOML.parse(
          readFileSync(resolve(root, path), 'utf8'),
        ) as {
          package?: { name: string };
        };
        if (!manifest.package) continue;
        packages.push(path);
        commands.push([
          'cargo',
          'fmt',
          '--manifest-path',
          path,
          '--package',
          manifest.package.name,
          ...(check ? ['--check'] : []),
        ]);
      }
      if (commands.length) tasks.push({ language, files: packages, commands });
    } else {
      const selected = files.filter((path) => {
        if (extensions[language].includes(extname(path))) return true;
        // Include hand-written extensionless shell scripts, but not wrappers
        // such as gradlew, which are excluded by .formatignore.
        return (
          language === 'shell' &&
          !extname(path) &&
          /^#![^\n]*\b(?:sh|bash|zsh|mksh)\b/.test(
            readFileSync(resolve(root, path), 'utf8').slice(0, 256),
          )
        );
      });
      if (!selected.length) continue;
      const command = list ? [] : toolCommand(language, check);
      // Bound argv size for platforms with small command-line limits. Prefix
      // paths so a source file beginning with '-' cannot become an option.
      const commands: string[][] = [];
      for (let i = 0; i < selected.length; i += 50) {
        commands.push([
          ...command,
          ...selected.slice(i, i + 50).map((path) => `./${path}`),
        ]);
      }
      tasks.push({ language, files: selected, commands });
    }
  }

  if (list) {
    for (const task of tasks) {
      console.log(`[${task.language}]\n${task.files.join('\n')}`);
    }
    return 0;
  }

  // Validate every required tool before allowing any formatter to write files.
  const missing = new Set<string>();
  for (const task of tasks) {
    const command = task.commands[0]!;
    if (!Bun.which(command[0]!)) missing.add(command[0]!);
    if (command[1] === '-jar' && !(await Bun.file(command[2]!).exists())) {
      missing.add(command[2]!);
    }
    if (task.language === 'rust' && Bun.which('cargo')) {
      const probe = Bun.spawnSync(['cargo', 'fmt', '--version'], {
        cwd: root,
        stdout: 'ignore',
        stderr: 'ignore',
      });
      if (probe.exitCode !== 0)
        missing.add('rustfmt (rustup component add rustfmt)');
    }
    if (
      task.language === 'swift' &&
      command[0] === 'swift' &&
      Bun.which('swift')
    ) {
      const probe = Bun.spawnSync(['swift', 'format', '--help'], {
        cwd: root,
        stdout: 'ignore',
        stderr: 'ignore',
      });
      if (probe.exitCode !== 0)
        missing.add(
          'swift-format (or a Swift toolchain providing swift format)',
        );
    }
  }
  if (missing.size) {
    throw new Error(
      `Missing formatters:\n${[...missing].map((tool) => `  ${tool}`).join('\n')}\nSee CONTRIBUTING.md#formatting for setup, or select a language with --only.`,
    );
  }

  let failed = false;
  for (const task of tasks) {
    console.log(
      `[${task.language}] ${check ? 'Checking' : 'Formatting'} ${task.files.length} ${task.language === 'rust' ? 'packages' : 'files'}`,
    );
    for (const command of task.commands) {
      const child = Bun.spawn(command, {
        cwd: root,
        stdin: 'ignore',
        stdout: 'inherit',
        stderr: 'inherit',
      });
      if ((await child.exited) !== 0) failed = true;
    }
  }
  return failed ? 1 : 0;
}

if (import.meta.main) {
  try {
    process.exitCode = await main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
