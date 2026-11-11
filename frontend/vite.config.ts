import { defineConfig } from "vitest/config";

export default defineConfig({
  clearScreen: false,
  esbuild: {
    jsx: "automatic",
    jsxImportSource: "preact",
  },
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    // The preview backend loads example designs from the repository root.
    fs: { allow: [".."] },
  },
  test: {
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
  },
});
