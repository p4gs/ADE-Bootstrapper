/**
 * Safe subprocess execution. Argv arrays only — no shell, no interpolation.
 * (Constraint: external input can never reach a shell string.)
 */
import type { ExecFn, ExecResult } from "./types.ts";

export const realExec: ExecFn = async (argv, opts): Promise<ExecResult> => {
  if (argv.length === 0) {
    return { code: 127, stdout: "", stderr: "empty argv" };
  }
  try {
    const proc = Bun.spawn(argv, {
      cwd: opts?.cwd,
      stdin: opts?.stdin !== undefined ? new TextEncoder().encode(opts.stdin) : undefined,
      stdout: "pipe",
      stderr: "pipe",
    });
    const [stdout, stderr, code] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ]);
    return { code, stdout, stderr };
  } catch (error) {
    return { code: 127, stdout: "", stderr: String(error) };
  }
};

export const realWhich = (name: string): string | null => Bun.which(name);
