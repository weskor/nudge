// Terminal helpers for interactive commands (setup, doctor). Prompts and progress go to stderr.

export class Cancelled extends Error {
  override name = "Cancelled";
}

const useColor = process.stderr.isTTY === true && !process.env.NO_COLOR;
const paint = (code: string) => (text: string) => (useColor ? `\x1b[${code}m${text}\x1b[0m` : text);

export const style = { green: paint("32"), red: paint("31"), yellow: paint("33"), dim: paint("2"), bold: paint("1") };
export const mark = { ok: style.green("✓"), fail: style.red("✗"), warn: style.yellow("!") };

export function say(line = ""): void {
  process.stderr.write(`${line}\n`);
}

/**
 * Read one line from the terminal in raw mode. `hidden` echoes nothing (for secrets).
 * Ctrl-C rejects with Cancelled.
 */
export async function promptLine(question: string, hidden = false): Promise<string> {
  const stdin = process.stdin;
  process.stderr.write(`${question} ${style.dim("›")} `);
  stdin.setRawMode(true);
  stdin.setEncoding("utf8");
  stdin.resume();
  try {
    return await new Promise<string>((resolve, reject) => {
      let value = "";
      const onData = (chunk: string) => {
        for (const char of chunk) {
          if (char === "\r" || char === "\n" || char === "\u0003") {
            stdin.off("data", onData);
            process.stderr.write("\n");
            if (char === "\u0003") reject(new Cancelled("cancelled"));
            else resolve(value.trim());
            return;
          }
          if (char === "\u007f" || char === "\b") {
            if (value && !hidden) process.stderr.write("\b \b");
            value = value.slice(0, -1);
          } else if (char >= " ") {
            value += char;
            if (!hidden) process.stderr.write(char);
          }
        }
      };
      stdin.on("data", onData);
    });
  } finally {
    stdin.setRawMode(false);
    stdin.pause();
  }
}

/** Yes/no question; Enter picks the default. */
export async function confirm(question: string, defaultYes: boolean): Promise<boolean> {
  for (;;) {
    const answer = (await promptLine(`${question} ${style.dim(defaultYes ? "[Y/n]" : "[y/N]")}`)).toLowerCase();
    if (answer === "") return defaultYes;
    if (answer === "y" || answer === "yes") return true;
    if (answer === "n" || answer === "no") return false;
  }
}

/** Wait for any key; used before a herdr pane closes so its output stays readable. */
export async function waitForKey(message = "Press any key to close"): Promise<void> {
  if (!process.stdin.isTTY) return;
  process.stderr.write(style.dim(message));
  const stdin = process.stdin;
  stdin.setRawMode(true);
  stdin.resume();
  await new Promise<void>((resolve) => stdin.once("data", () => resolve()));
  stdin.setRawMode(false);
  stdin.pause();
  process.stderr.write("\n");
}

/**
 * Recognizable but safe preview of a secret: the scheme, then dots and the last 4 characters.
 * Values shorter than 16 characters (after the scheme) show no characters at all.
 * `Bearer abcdefghijklmnopqrstuvwxyz` → `Bearer ••••••••wxyz`.
 */
export function maskSecret(value: string): string {
  const match = /^([A-Za-z]+)\s+(\S.*)$/.exec(value);
  const [scheme, secret] = match ? [`${match[1]} `, match[2]!] : ["", value];
  const dots = "••••••••";
  return `${scheme}${secret.length >= 16 ? `${dots}${secret.slice(-4)}` : dots}`;
}
