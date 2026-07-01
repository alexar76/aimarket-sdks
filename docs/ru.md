# aimarket SDK — каноническое сводное руководство (Dart / TypeScript / Rust)

Клиентские SDK на родных языках для **AIMarket Protocol v2** — JSON/HTTP-маркетплейса, где ваше приложение может **находить**, **оплачивать** и **вызывать** AI-возможности, обслуживаемые независимыми hub'ами. Три SDK, описанные здесь (Dart, TypeScript, Rust), реализуют **один и тот же 5-фазный цикл**, **одни и те же модели данных** и **одну и ту же подпись Ed25519**, удерживаемые в синхроне guard'ом паритета экосистемы в CI. Существует и четвёртый, архитектурно иной SDK для Python — см. [руководство по Python](https://github.com/alexar76/aimarket-agent/blob/main/docs/en.md).

> **Референсный hub:** [modelmarket.dev](https://modelmarket.dev) · **Factory:** [magic-ai-factory.com](https://magic-ai-factory.com) · **Оракулы:** [oracles.modelmarket.dev](https://oracles.modelmarket.dev) · **Политика версий:** [`docs/sdk-version-policy.md`](../../docs/sdk-version-policy.md)

Это **флагманское руководство по SDK**. Оно техническое, насыщено таблицами и построено вокруг примеров. Каждая сигнатура, заголовок, порт и адрес ниже взяты из живого исходного кода в `aimarket-sdks/`.

---

## 1. Что это такое

AIMarket Protocol v2 — это универсальный JSON/HTTP-маркетплейс AI-возможностей. Продавцы публикуют возможности за hub'ами; потребители находят их по **intent**, открывают **предоплаченный платёжный канал**, **вызывают** возможность (оплачивая каждый вызов из канала) и **завершают расчёт**, получая возврат всего, что не было израсходовано. Возможности могут выполняться внутри **TEE** (доверенного анклава исполнения), и протокол позволяет криптографически проверить это до и после вызова. Hub'ы **федерируются**, поэтому один hub может маршрутизировать вызов к возможности, размещённой на другом.

Четыре SDK:

| SDK | Пакет | Целевые среды выполнения |
|-----|---------|-----------------|
| **Dart** | `aimarket_agent` | Flutter desktop (macOS/Windows/Linux), Dart-серверы |
| **TypeScript** | `@aimarket/agent` | Electron, Node.js-серверы, веб-приложения |
| **Rust** | `aimarket-agent` (crate) | Tauri, нативные CLI-инструменты |
| **Python** | `aimarket-agent` (PyPI) | LangChain / серверные агенты / CLI — *отдельный, stateless, без кошелька* → [руководство по Python](https://github.com/alexar76/aimarket-agent/blob/main/docs/en.md) |

Живые URL, на которые вы будете ссылаться:

- **Референсный hub (точка входа для потребителя):** `https://modelmarket.dev` — AIMarket Protocol v2. Локальная разработка: `http://localhost:9083`.
- **Приложение / сайт Factory:** `https://magic-ai-factory.com` — где возможности создаются и публикуются.
- **Портал оракулов:** `https://oracles.modelmarket.dev` — семнадцать живых оракулов с проверяемой математикой, все доступны как возможности.

Используйте `https://modelmarket.dev` как канонический `hubUrl` в собственном коде, если только вы не нацеливаетесь на self-hosted или федеративный hub.

---

## 2. Выберите свой SDK и установите

| Среда выполнения | Пакет | Установка | Линия версий |
|---------|---------|---------|--------------|
| **Flutter / Dart** | `aimarket_agent` | `dart pub add aimarket_agent` | **0.1.x** |
| **Electron / Node.js** | `@aimarket/agent` | `npm install @aimarket/agent` | **0.1.x** |
| **Tauri / Rust** | `aimarket-agent` (crate) | `cargo add aimarket-agent` | **0.1.x** |
| **Сервер / LangChain / CLI** | `aimarket-agent` (PyPI) | `pip install aimarket-agent` → [руководство по Python](https://github.com/alexar76/aimarket-agent/blob/main/docs/en.md) | **2.1.x** |

> **Две линии версий — намеренно.** Dart / TypeScript / Rust — это синхронизированное кросс-платформенное семейство на **0.1.x**; Python-агент — более старый, отдельный пакет на **2.1.x** в PyPI. Оба нацелены на AIMarket Protocol v2. См. [`docs/sdk-version-policy.md`](../../docs/sdk-version-policy.md) — не пытайтесь «исправить» это расхождение.

### Установка из реестра (рекомендуется)

```yaml
# Dart — pubspec.yaml (published on pub.dev)
dependencies:
  aimarket_agent: ^0.1.0
# or:  dart pub add aimarket_agent
```

```bash
# TypeScript — published on npm
npm install @aimarket/agent
```

```toml
# Rust — Cargo.toml (published on crates.io)
[dependencies]
aimarket-agent = "0.1.0"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"
# or:  cargo add aimarket-agent
```

### Установка из monorepo / git (локальная разработка)

```yaml
# Dart — path or git dependency
dependencies:
  aimarket_agent:
    path: ../aimarket-sdks/dart
  # or:
  # aimarket_agent:
  #   git: { url: https://github.com/alexar76/aimarket-sdks, path: dart }
```

```bash
# TypeScript — build from the monorepo
git clone https://github.com/alexar76/aimarket-sdks
cd aimarket-sdks/typescript && npm install && npm run build
```

```toml
# Rust — path dependency on the monorepo crate
aimarket-agent = { path = "../aimarket-sdks/rust" }
```

---

## 3. Аутентификация — правда об Ed25519

**Ключ кошелька, которым подписывают эти SDK, — это seed Ed25519, а не приватный ключ Ethereum/secp256k1.** Это самое важное, что нужно понять правильно.

- `walletKey` — это **64-символьная hex-строка = 32-байтовый seed Ed25519**. Signer превращает её в пару ключей Ed25519.
- Если строка **не** является 64-символьным hex, она хешируется через SHA-256 в 32-байтовый seed (dev-фолбэк — никогда не используйте его в продакшене).
- Каждый запрос **invoke** подписывается над канонической строкой `channel:<id>|capability:<id>|affiliate:<affil>`, давая подпись в формате `ed25519:<base64>`, отправляемую в заголовке `X-Market-Signature`.

Signer для Dart (`dart/lib/src/signer.dart`) делает все три пункта явными:

```dart
// signedHeaders() — what every invoke sends:
final canonical = 'channel:$channelId|capability:$capabilityId|affiliate:$affiliate';
return {
  'X-Payment-Channel': channelId,
  'X-AIMarket-Affiliate': affiliate,
  'X-Market-Signature': signCanonical(canonical),  // 'ed25519:<base64>'
};
```

```dart
// _parseSeedBytes() — 64-char hex => raw 32-byte seed; otherwise SHA-256(input).
final normalized = hexOrDev.startsWith('0x') ? hexOrDev.substring(2) : hexOrDev;
if (normalized.length == 64 && RegExp(r'^[0-9a-fA-F]+$').hasMatch(normalized)) {
  // use the hex bytes directly as the Ed25519 seed
}
// else: sha256(input)  ← dev fallback only
```

### Безопасная загрузка ключа (никогда не зашивайте в код)

Извлекайте seed из платформенного keychain / защищённого хранилища во время выполнения. Не коммитьте его, не вшивайте в бинарник.

```dart
// Dart / Flutter — e.g. flutter_secure_storage
final walletKey = await secureStorage.read(key: 'aimarket_wallet_seed');
final agent = AimarketAgent(hubUrl: 'https://modelmarket.dev', walletKey: walletKey!);
```

```ts
// TypeScript / Electron — e.g. safeStorage / keytar
const walletKey = await loadWalletSeedFromKeychain();
const agent = new AimarketAgent({ hubUrl: 'https://modelmarket.dev', walletKey });
```

```rust
// Rust / Tauri — e.g. the OS keyring
let wallet_key = load_wallet_seed_from_keyring()?;
let agent = AimarketAgent::new("https://modelmarket.dev", &wallet_key);
```

### Опциональный вторичный ключ (EIP-712 / secp256k1)

Существует **второй, опциональный** ключ, используемый только для **on-chain-списаний с канала** через `AIMarketEscrow.debitChannel` — подпись EIP-712 над ключом secp256k1 (`eip712:0x<r><s><v>`). В Dart это `MarketSigner(ethereumPrivateKeyHex: ...)`, потребляемый методом `signDebitAuthorization(...)`. **Большинству потребителей он не нужен** — поток channel/invoke/settle целиком работает на seed Ed25519. Обращайтесь к нему только если завершаете расчёт по каналам напрямую on-chain.

> **Не** называйте `walletKey` «приватным ключом EVM/Ethereum». Это seed Ed25519. Ключ secp256k1/EIP-712 — это отдельный, опциональный ввод.

---

## 4. 5-фазный жизненный цикл стоимости

Каждый SDK реализует одни и те же пять фаз против одних и тех же endpoint'ов hub'а.

| # | Фаза | Endpoint hub v2 | Метод (показан Dart) | Что делает |
|---|-------|-----------------|---------------------|--------------|
| 1 | **Discovery** | `GET /.well-known/ai-market.json` → `GET /ai-market/v2/search` | `discover(...)` | находит возможности, соответствующие intent + бюджету |
| 2 | **Channel** | `POST /ai-market/v2/channel/open` | `openChannel(depositUsd, token, chain)` | открывает предоплаченный платёжный канал |
| 3 | **Invoke** | `POST /ai-market/v2/invoke` (заголовки `X-Payment-Channel`, `X-Market-Signature`) | `invoke(...)` | вызывает возможность, оплачивает из канала |
| 4 | **Settle** | `POST /ai-market/v2/channel/close` | `closeChannel(channelId)` | закрывает канал, возвращает неизрасходованный баланс |
| 5 | **Verify** | *(локально)* | `verifyTeeAttestation(...)` / `verifyTeeReceipt(...)` | аттестация TEE перед отправкой + receipt после |

`runOnce(...)` — это удобный метод, который выполняет все пять фаз и возвращает `BillOfMaterials`.

### Поверхность методов (по языкам)

| Фаза | Dart | TypeScript | Rust |
|-------|------|------------|------|
| Discover | `discover({intent, budget?, limit=5, category?})` | `discover({ intent, budget?, limit?, category? })` | `discover(intent, budget, limit, category)` |
| Channel | `openChannel(depositUsd, {token='USDT', chain='base'})` | `openChannel(depositUsd, token='USDT', chain='base')` | `open_channel(deposit_usd, token, chain)` |
| Invoke | `invoke({capabilityId, input, channelId, sourceHub?, productId?, verifyTee=true, attestation?})` | `invoke({ capabilityId, input, channelId, productId?, sourceHub?, verifyTee?, attestation? })` | `invoke(capability_id, input, channel_id, product_id, source_hub)` |
| Settle | `closeChannel(channelId)` | `closeChannel(channelId)` | `close_channel(channel_id)` |
| Полный цикл | `runOnce({intent, input, depositUsd=5.00, category?})` | `runOnce({ intent, input, depositUsd?, category? })` | `run_once(intent, input, deposit_usd, category)` |

> **Именование полей результата различается по идиоматике языка.** Dart использует camelCase (`result.priceUsd`, `result.teeVerified`, `result.teeReceipt`); TypeScript и Rust используют snake_case (`result.price_usd`, `result.tee_verified`, `result.tee_receipt`). JSON на проводе везде snake_case. Шаги плана: Dart предоставляет `step.capability.id`; TS/Rust предоставляют `step.capability.capability_id`.

### Один вызов: `runOnce(...)`

```dart
// Dart
final agent = AimarketAgent(hubUrl: 'https://modelmarket.dev', walletKey: loadYourWalletKey());
final bom = await agent.runOnce(
  intent: 'pdf summarization',
  input: {'text': '...'},
  depositUsd: 5.00,
  category: 'productivity',
);
print('task=${bom.task} protocol=${bom.protocolVersion} spent=\$${bom.totalSpentUsd}');
agent.dispose();
```

```ts
// TypeScript
const agent = new AimarketAgent({ hubUrl: 'https://modelmarket.dev', walletKey: loadYourWalletKey() });
const bom = await agent.runOnce({
  intent: 'pdf summarization',
  input: { text: '...' },
  depositUsd: 5.0,
  category: 'productivity',
});
console.log(`task=${bom.task} protocol=${bom.protocol_version} spent=$${bom.total_spent_usd}`);
agent.dispose();
```

```rust
// Rust
use aimarket_agent::AimarketAgent;
use serde_json::json;

let agent = AimarketAgent::new("https://modelmarket.dev", &load_your_wallet_key());
let bom = agent
    .run_once("pdf summarization", json!({ "text": "..." }), Some(5.00), Some("productivity"))
    .await?;
println!("task={} protocol={} spent=${}", bom.task, bom.protocol_version, bom.total_spent_usd);
agent.dispose();
```

### Ручной цикл: Discovery → Channel → Invoke → Settle

```dart
// Dart
final agent = AimarketAgent(hubUrl: 'https://modelmarket.dev', walletKey: loadYourWalletKey());

final plan    = await agent.discover(intent: 'pdf summarization', budget: 10.00, category: 'productivity');
final channel = await agent.openChannel(5.00);                       // token: 'USDT', chain: 'base'
final result  = await agent.invoke(
  capabilityId: plan.first.capability.id,
  input: {'text': '...'},
  channelId: channel.id,
  verifyTee: true,
);
print('ok=${result.success} cost=\$${result.priceUsd} tee=${result.teeVerified}');

final settlement = await agent.closeChannel(channel.id);
print('spent=\$${settlement.totalSpentUsd} refund=\$${settlement.refundUsd}');
agent.dispose();
```

```ts
// TypeScript
const agent = new AimarketAgent({ hubUrl: 'https://modelmarket.dev', walletKey: loadYourWalletKey() });

const plan    = await agent.discover({ intent: 'pdf summarization', budget: 10.0, category: 'productivity' });
const channel = await agent.openChannel(5.0);                        // token 'USDT', chain 'base'
const result  = await agent.invoke({
  capabilityId: plan[0].capability.capability_id,
  input: { text: '...' },
  channelId: channel.channel_id,
  verifyTee: true,
});
console.log(`ok=${result.success} cost=$${result.price_usd} tee=${result.tee_verified}`);

const settlement = await agent.closeChannel(channel.channel_id);
console.log(`spent=$${settlement.total_spent_usd} refund=$${settlement.refund_usd}`);
agent.dispose();
```

```rust
// Rust
use aimarket_agent::AimarketAgent;
use serde_json::json;

let agent = AimarketAgent::new("https://modelmarket.dev", &load_your_wallet_key());

let plan    = agent.discover("pdf summarization", Some(10.0), Some(5), Some("productivity")).await?;
let channel = agent.open_channel(5.0, "USDT", "base").await?;
let result  = agent
    .invoke(&plan[0].capability.capability_id, json!({ "text": "..." }), &channel.channel_id, None, None)
    .await?;
println!("ok={} cost=${} tee={}", result.success, result.price_usd, result.tee_verified);

let settlement = agent.close_channel(&channel.channel_id).await?;
println!("spent=${} refund=${}", settlement.total_spent_usd, settlement.refund_usd);
agent.dispose();
```

> **Дефолты, которые стоит знать:** `openChannel` по умолчанию использует `token='USDT'`, `chain='base'`. `runOnce` по умолчанию `depositUsd=5.00`. На каждый запрос `timeout` равен 30 с, а `maxRetries` равен 3 (повторяются только сетевые ошибки и таймауты, с backoff 1с/2с/4с; ошибки 402/403 и ошибки протокола пробрасываются немедленно). Дефолтный affiliate зависит от языка: `aimarket-sdk-dart`, `aimarket-sdk-ts`, `aimarket-sdk-rust`.

---

## 5. Проверка TEE

Если возможность выполняется в доверенном анклаве, вы можете это доказать — **до** отправки данных и **после** получения вывода. Паттерн таков: зарегистрировать ожидаемый хеш кода, проверить аттестацию, прервать при неудаче, затем проверить receipt.

```dart
// Dart — register a known-good code hash, then verify the attestation BEFORE sending
agent.trustCodeHash('cap-id', 'sha256:a1b2c3d4...');

if (!agent.verifyTeeAttestation(attestation, 'cap-id')) {
  // The capability is NOT running the trusted code in a real enclave — abort.
  return;
}

final result = await agent.invoke(
  capabilityId: 'cap-id',
  input: input,
  channelId: channel.id,
  verifyTee: false,   // already verified the attestation above
);

// Verify the receipt AFTER: proves the output came out of the attested enclave.
if (result.teeReceipt != null &&
    !agent.verifyTeeReceipt(result.teeReceipt!, json.encode(input), json.encode(result.output))) {
  // The output may NOT have been produced in the attested enclave — treat as untrusted.
}
```

```ts
// TypeScript
agent.trustCodeHash('cap-id', 'sha256:a1b2c3d4...');

if (!agent.verifyTeeAttestation(attestation, 'cap-id')) return; // abort — not a verified enclave

const result = await agent.invoke({
  capabilityId: 'cap-id',
  input,
  channelId: channel.channel_id,
  verifyTee: false,
});

if (result.tee_receipt &&
    !agent.verifyTeeReceipt(result.tee_receipt, JSON.stringify(input), JSON.stringify(result.output))) {
  // output not verifiably produced in the attested enclave
}
```

```rust
// Rust — note: the Rust agent exposes verify_tee_attestation + trust_code_hash.
agent.trust_code_hash("cap-id", "sha256:a1b2c3d4...");

if !agent.verify_tee_attestation(&attestation, "cap-id") {
    // abort — not a verified enclave
    return Ok(());
}
let result = agent.invoke("cap-id", input, &channel.channel_id, None, None).await?;
```

Два взаимодополняющих способа проверки плюс встроенная предполётная проверка:

- **`verifyTeeAttestation(attestation, capabilityId)`** — перед отправкой. Проверяет хеш кода анклава и подпись против вашего доверенного набора. Возвращает `false`, если анклаву нельзя доверять.
- **`verifyTeeReceipt(receipt, sentInput, receivedOutput)`** *(Dart / TypeScript)* — после получения. Доказывает, что вывод был произведён в аттестованном анклаве из точно того ввода, который вы отправили.
- **Встроенная предпроверка:** когда вы передаёте `attestation` в `invoke(..., verifyTee: true)`, SDK проверяет её **до передачи какого-либо ввода** и **fail closed** — плохая аттестация прерывает вызов (Dart/TS поднимают исключение безопасности), так что чувствительные данные никогда не достигают непроверенного анклава.

> **Почему это важно:** без проверки вы полагаетесь на слово продавца, что код, который, как вы думаете, выполняется, действительно выполняется — и в настоящем анклаве. Проверяйте аттестацию, прежде чем раскрыть ввод; проверяйте receipt, прежде чем доверять выводу.

---

## 6. Федерация — вызов возможности на другом hub'е

Hub'ы федерируются. Чтобы вызвать возможность, которая живёт на другом hub'е, передайте `sourceHub` (и обычно `productId`). Ваш канал и подпись остаются на вашем домашнем hub'е; он маршрутизирует вызов.

```dart
// Dart
final result = await agent.invoke(
  capabilityId: 'cap',
  input: input,
  channelId: channel.id,
  sourceHub: 'https://hub-seller.example.com',
  productId: 'cap',
);
```

```ts
// TypeScript
const result = await agent.invoke({
  capabilityId: 'cap',
  input,
  channelId: channel.channel_id,
  sourceHub: 'https://hub-seller.example.com',
  productId: 'cap',
});
```

```rust
// Rust — product_id and source_hub are the 4th and 5th args.
let result = agent
    .invoke("cap", input, &channel.channel_id, Some("cap"), Some("https://hub-seller.example.com"))
    .await?;
```

`runOnce(...)` уже делает это автоматически: он прокидывает `capability.sourceHub` и `capability.productId` каждого шага плана в invoke, поэтому федеративные результаты маршрутизируются корректно без лишней работы.

---

## 7. Обработка ошибок

SDK сопоставляют HTTP-статус-коды с типизированными ошибками и понятными путями восстановления.

| Ошибка | Код | Значение | Восстановление |
|-------|------|---------|----------|
| Discovery failed | 4xx/5xx | hub недоступен / некорректный intent | повторить, проверить URL hub'а |
| Channels unavailable | 404 | у hub'а нет channel-плагина | использовать другой hub или `runOnce` |
| Payment required | 402 | канал исчерпан или истёк | открыть канал побольше, повторить |
| Safety blocked | 403 | ввод сработал на safety-gate | пересмотреть / вычистить ввод (PII), повторить |
| TEE not verified | 200 | аттестация/receipt не прошли | проверить вручную, считать вывод недоверенным |

### Типы исключений

- **Dart:** `AimarketException` (базовый) с подклассами `AimarketNetworkException`, `AimarketPaymentException` (402), `AimarketSafetyException` (403, предоставляет `.reason`). Изучите `.message` и `.statusCode`.
- **TypeScript:** та же иерархия — `AimarketException`, `AimarketNetworkException`, `AimarketPaymentException`, `AimarketSafetyException`.
- **Rust:** enum `AimarketError` — `Network(..)`, `Payment(..)`, `Safety(..)`, `Protocol(..)`.

```dart
// Dart — recover from a depleted channel
try {
  final result = await agent.invoke(capabilityId: 'cap', input: input, channelId: channel.id);
} on AimarketPaymentException {
  final fresh = await agent.openChannel(10.00);   // bigger deposit
  // retry with fresh.id …
} on AimarketSafetyException catch (e) {
  print('blocked: ${e.reason}');                  // scrub input, retry
} on AimarketException catch (e) {
  if (e.message.contains('depleted')) { /* … */ }
}
```

Учтите, что non-2xx, который не является ни 402, ни 403, **не** бросает исключение из `invoke` — он возвращает `InvokeResult` с `success: false` и установленным `error`, поэтому всегда проверяйте `result.success`, даже когда исключение не поднято.

---

## 8. Продвинутое

### Переиспользование канала + паттерн service-обёртки

Откройте канал один раз и переиспользуйте его для множества вызовов. SDK уже кэширует каналы по ключу `deposit:token:chain` и переиспользует один, пока он **не истёк и сохраняет > 50% баланса**; ниже этого порога он прозрачно открывает новый. В долгоживущем приложении оберните агента в сервис, который держит один канал, пока его баланс не упадёт ниже порога, а затем закрывает и переоткрывает — и всегда вызывайте `dispose()` / закрывайте канал при завершении работы, чтобы неизрасходованные средства были возвращены.

```dart
// Dart — production service wrapper (condensed)
class MarketplaceService {
  final AimarketAgent _agent;
  Channel? _channel;
  MarketplaceService(String hubUrl, String walletKey)
      : _agent = AimarketAgent(hubUrl: hubUrl, walletKey: walletKey, affiliate: 'my-app-v1');

  Future<Channel> _ensureChannel({double minBalance = 1.0}) async {
    if (_channel != null && _channel!.balanceUsd >= minBalance) return _channel!;
    if (_channel != null) { try { await _agent.closeChannel(_channel!.id); } catch (_) {} }
    _channel = await _agent.openChannel(5.00);
    return _channel!;
  }

  Future<InvokeResult> call(String capabilityId, Map<String, dynamic> input) async {
    final ch = await _ensureChannel();
    return _agent.invoke(capabilityId: capabilityId, input: input, channelId: ch.id);
  }

  Future<void> dispose() async {
    if (_channel != null) { try { await _agent.closeChannel(_channel!.id); } catch (_) {} }
    _agent.dispose();
  }
}
```

### Удаляйте PII перед удалёнными вызовами

Возможности выполняются на чужом hub'е. **Очищайте данные на стороне клиента** — не полагайтесь на то, что hub или продавец вычистят их за вас. Удаляйте прямые идентификаторы (`email`, `phone`, имена, `resume_text`, …) и одностороннее хешируйте всё, что необходимо коррелировать (например, `company` → `company_hash`).

```dart
Map<String, dynamic> stripPii(Map<String, dynamic> data) {
  final out = Map<String, dynamic>.from(data);
  for (final k in ['candidate_name', 'email', 'phone', 'linkedin_url', 'resume_text']) {
    out.remove(k);
  }
  if (out.containsKey('company')) {
    out['company_hash'] = hashField(out['company'].toString());
    out.remove('company');
  }
  return out;
}
// … then: agent.invoke(capabilityId: cap, input: stripPii(raw), channelId: ch.id);
```

### Конвейеры из нескольких возможностей

Для рабочих процессов, которые компонуют несколько возможностей в DAG поверх одного общего канала — с оценкой стоимости по шагам и fan-out — см. [пример интеграции capability-composer](https://github.com/alexar76/aimarket-desktop/blob/main/capability-composer/docs/sdk-integration.md). `invokeBatch(...)` выполняет множество возможностей на одном канале (Dart/TS делают fan-out конкурентно; Rust выполняет их последовательно) и является строительным блоком для таких конвейеров.

---

## 9. Версионирование и поддержка

Dart / TypeScript / Rust — это синхронизированное кросс-платформенное семейство на **0.1.x**; Python-агент — отдельный пакет **2.1.x** в PyPI. Версии отслеживают *пакет*, а не протокол — все четыре нацелены на AIMarket Protocol v2. Кросс-языковые тестовые векторы живут в `aimarket-sdks/test-vectors/`, а guard паритета экосистемы удерживает три скомпилированных SDK на совпадающих версиях и формах моделей в CI. Полные детали: [`docs/sdk-version-policy.md`](../../docs/sdk-version-policy.md).

---

## 10. Связанная документация

- [aimarket-sdks README](../README.md) — быстрый старт для всех трёх скомпилированных SDK
- [Dart SDK README](../dart/README.md) · [TypeScript SDK README](../typescript/README.md) · [Rust SDK README](../rust/README.md)
- [Руководство по Python SDK](https://github.com/alexar76/aimarket-agent/blob/main/docs/en.md) — stateless-потребитель без кошелька
- [Политика версий SDK](../../docs/sdk-version-policy.md)
- [Оракулы — возможности с проверяемой математикой](https://github.com/alexar76/oracles/blob/main/docs/en.md)
- Примеры интеграции: [interview-prep-coach](https://github.com/alexar76/aimarket-desktop/blob/main/interview-prep-coach/docs/sdk-integration.md) · [cold-outreach-coach](https://github.com/alexar76/aimarket-desktop/blob/main/cold-outreach-coach/docs/sdk-integration.md) · [capability-composer](https://github.com/alexar76/aimarket-desktop/blob/main/capability-composer/docs/sdk-integration.md)
- Живые ресурсы: [modelmarket.dev](https://modelmarket.dev) · [magic-ai-factory.com](https://magic-ai-factory.com) · [oracles.modelmarket.dev](https://oracles.modelmarket.dev)

---

🇬🇧 [English](en.md) · 🇷🇺 [Русский](ru.md) · 🇪🇸 [Español](es.md)
