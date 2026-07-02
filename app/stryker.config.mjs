/** @type {import('@stryker-mutator/core').PartialStrykerOptions} */
export default {
  testRunner: 'vitest',
  // n-cursor запускає Stryker через `npx @stryker-mutator/core@latest` із tmp-каталогу;
  // явний plugins list змушує Stryker зарезолвити vitest-runner з нашого hoisted node_modules.
  plugins: ['@stryker-mutator/vitest-runner', './stryker-vue-macros-ignorer.mjs'],
  vitest: { configFile: 'vitest.config.js' },
  // inPlace avoids hoisted-node_modules issues in a Bun monorepo sandbox
  inPlace: true,
  // incremental: зберігає результати між запусками, відновлює після SIGURG/kill
  incremental: true,
  incrementalFile: 'reports/stryker/incremental.json',
  mutate: [
    'src/**/*.{js,vue}',
    '!src/**/*.test.js',
    // app-shell / bootstrap — Quasar.use, mount('#app'); за каноном tauri-rule
    // platform glue не покривається unit-mutation testing.
    '!src/main.js',
    // test infrastructure (mount helpers) — мутувати безглуздо, її «тест» — це
    // сам факт, що інші тести працюють.
    '!src/test-utils/**'
  ],
  // concurrency:1 prevents parallel workers from competing over inPlace source files
  concurrency: 1,
  // 60 s per mutant — happy-dom SFC compilation needs headroom
  timeoutMS: 60000,
  reporters: ['json', 'clear-text'],
  jsonReporter: { fileName: 'reports/stryker/mutation.json' },
  tempDirName: 'reports/stryker/.tmp',
  coverageAnalysis: 'perTest',
  ignorers: ['vue-macros']
}
