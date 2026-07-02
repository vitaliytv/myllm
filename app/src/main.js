import '@quasar/extras/material-symbols-outlined/material-symbols-outlined.css'
import { Quasar } from 'quasar'
import iconSet from 'quasar/icon-set/material-symbols-outlined'
import 'quasar/src/css/index.sass'
import App from './App.vue'

createApp(App)
  .use(Quasar, {
    config: {
      dark: 'auto'
    },
    iconSet
  })
  .mount('#app')
