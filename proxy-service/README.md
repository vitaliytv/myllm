# myllm-proxy-service

Безінтерфейсний (headless) зворотний проксі omlx — раніше жив усередині
Tauri-застосунку `myllm` (кнопка «Підключити», картка «Черга зараз», картка
«Історія запитів»); винесено в окремий persistent-сервіс, щоб працювати без
відкритого вікна, керовано macOS `launchd`.

Функціонал — повний паритет з колишнім проксі в `myllm`:

- зворотний проксі `/v1/*` → upstream omlx, зі стрімінгом SSE без буферизації;
- компресія тіла запиту перед форвардом (`compress.rs`, minify + truncate
  великих блоків, tool-calls/response_format лишаються byte-exact) — Rust-
  дзеркало канонічної логіки `@7n/llm-lib/lib/internal/compress-context.mjs`
  (клієнти БЕЗ llm-lib досі йдуть через цей проксі, тож компресія лишається
  тут як debug/legacy-шлях, а не єдине джерело правди);
- історія запитів (`requests.jsonl`) + резолв процесу-клієнта (`client_info.rs`,
  macOS-only, через таблицю TCP-сокетів + `libproc`);
- кореляція з ланцюжками `@7n/llm-lib` (`x-chain-*` заголовки + fallback
  `prompt_hash`, дзеркало контракту `llm-lib/lib/chain.mjs::promptHash`);
- admin-клієнт до upstream-сервера (`/admin/api/login|stats|global-settings`).

## Конфіг (env)

| Змінна | Дефолт | Призначення |
| --- | --- | --- |
| `MYLLM_PROXY_UPSTREAM_URL` | `http://127.0.0.1:8000` | куди форвардиться `/v1/*` |
| `MYLLM_PROXY_PORT` | `8088` | локальний порт проксі |
| `OMLX_API_KEY` | — | якщо задано, admin-сесія піднімається автоматично при старті |
| `MYLLM_PROXY_DATA_DIR` | `~/Library/Application Support/myllm-proxy-service` | де лежить `requests.jsonl` |

## Локальний admin-API (без GUI)

Замість колишньої Tauri-команди + UI-кнопок:

```bash
# Історія (той самий requests.jsonl, що показувала колишня вкладка)
curl http://127.0.0.1:8088/_proxy/history?limit=50
curl -X DELETE http://127.0.0.1:8088/_proxy/history

# Admin-сесія (якщо OMLX_API_KEY не заданий при старті)
curl -X POST http://127.0.0.1:8088/_proxy/admin/connect \
  -H 'content-type: application/json' \
  -d '{"baseUrl":"http://127.0.0.1:8000","apiKey":"<key>"}'
curl http://127.0.0.1:8088/_proxy/admin/stats
curl http://127.0.0.1:8088/_proxy/admin/global-settings
```

## Запуск як launchd-сервіс

```bash
./launchd/install.sh                 # білдить cargo build --release локально, реєструє + запускає LaunchAgent
./launchd/install-from-release.sh    # без Rust: завантажує universal-бінарник з GitHub Release (тег latest або конкретний)
./launchd/uninstall.sh               # зупиняє й видаляє LaunchAgent (requests.jsonl лишається)
```

CI (`.github/workflows/release.yml`, job `build-proxy-service`) на кожен тег `v*`
білдить universal (`aarch64`+`x86_64`, `lipo`) бінарник і кладе його як release
asset `myllm-proxy-service-universal-apple-darwin.tar.gz` — `install-from-release.sh`
саме його й завантажує (`gh release download`, потрібен встановлений `gh`). Бінарник
**не підписаний/нотаризований** (внутрішній інструмент) — якщо Gatekeeper блокує
запуск, скрипт друкує команду `xattr -d com.apple.quarantine` для ручного зняття
карантину.

Лог: `/tmp/myllm-proxy-service.log`. Перевірка стану:
`launchctl print gui/$(id -u)/com.nitra.myllm-proxy-service`.

## Розробка

```bash
cargo run -p myllm-proxy-service       # foreground, Ctrl-C зупиняє
cargo test -p myllm-proxy-service
cargo clippy -p myllm-proxy-service --all-targets --all-features -- -D warnings
```
