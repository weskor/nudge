import pkg from "../package.json" with { type: "json" };

/** nudge version; package.json and herdr-plugin.toml must agree (checked by the release workflow). */
export const VERSION: string = pkg.version;
