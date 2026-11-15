// Makes a new signing keypair.
//   private JWK -> server/.dev.vars (gitignored) and, for production: npx wrangler secret put LICENSE_SIGNING_KEY
//   public key  -> src-tauri/license-public.key (32 raw bytes, compiled into the app)
// Run once. Making a new pair later invalidates every key already sold.
import { generateKeyPairSync } from "node:crypto";
import { existsSync, writeFileSync } from "node:fs";

const pub = new URL("../../src-tauri/license-public.key", import.meta.url);
if (existsSync(pub) && !process.argv.includes("--force")) {
  console.error("license-public.key exists. Re-run with --force only if no keys have been sold.");
  process.exit(1);
}
const { privateKey, publicKey } = generateKeyPairSync("ed25519");
const jwk = privateKey.export({ format: "jwk" });
writeFileSync(pub, Buffer.from(publicKey.export({ format: "jwk" }).x, "base64url"));
writeFileSync(new URL("../.dev.vars", import.meta.url), `LICENSE_SIGNING_KEY='${JSON.stringify(jwk)}'\n`, { flag: "a" });
console.log("public key -> src-tauri/license-public.key\nprivate key -> server/.dev.vars");
