import { defineConfig } from "vitest/config";
import { cloudflareTest, readD1Migrations } from "@cloudflare/vitest-plugin";

export default defineConfig(async () => ({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: "./wrangler.jsonc" },
      miniflare: {
        bindings: {
          TEST_MIGRATIONS: await readD1Migrations("./migrations"),
          STRIPE_SECRET_KEY: "sk_test_x",
          STRIPE_WEBHOOK_SECRET: "whsec_test",
        },
      },
    }),
  ],
}));
