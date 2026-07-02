import { defineConfig, mergeConfig } from 'vitest/config'
import viteConfig from './vite.config.js'

export default mergeConfig(
  viteConfig({ command: 'serve', mode: 'test' }),
  defineConfig({
    test: {
      include: ['src/**/*.test.{js,mjs}', 'tests/**/*.test.{js,mjs}'],
      environment: 'happy-dom',
      // @7n/tauri-components ships .vue source; inline it so the vue plugin
      // compiles it instead of Node importing the raw .vue file.
      server: { deps: { inline: [/@7n\/tauri-components/] } },
      globals: false,
      coverage: { provider: 'v8', reporter: ['lcov', 'text-summary'] }
    }
  })
)
