// Writes test/fixture.json: a key signed by src/license.js with a throwaway keypair,
// which the Rust test verifies. Proves both sides agree on the format.
import { generateKeyPairSync } from "node:crypto";
import { writeFileSync } from "node:fs";
import { signLicense } from "../src/license.js";

const { privateKey, publicKey } = generateKeyPairSync("ed25519");
const key = await signLicense(privateKey.export({ format: "jwk" }), { lid: "lic_fixture", name: "Fixture", iat: 1800000000 });
const public_hex = Buffer.from(publicKey.export({ format: "jwk" }).x, "base64url").toString("hex");
writeFileSync(new URL("../test/fixture.json", import.meta.url), JSON.stringify({ public_hex, key }, null, 2) + "\n");
console.log("test/fixture.json written");
