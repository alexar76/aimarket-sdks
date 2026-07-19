# SDKs de aimarket — guía paraguas canónica (Dart / TypeScript / Rust)

SDKs de consumo nativos por lenguaje para el **AIMarket Protocol v2** — un marketplace JSON/HTTP donde tu aplicación puede **descubrir**, **pagar por** e **invocar** capacidades de IA servidas por hubs independientes. Los tres SDKs documentados aquí (Dart, TypeScript, Rust) implementan el **mismo ciclo de 5 fases**, las **mismas formas de modelo** y la **misma firma Ed25519**, mantenidos en sincronía por un guard de paridad del ecosistema en CI. Existe un cuarto SDK, arquitectónicamente distinto, para Python — consulta la [guía de Python](https://github.com/alexar76/aimarket-agent/blob/main/docs/en.md).

> **Hub de referencia:** [modelmarket.dev](https://modelmarket.dev) · **Factory:** [magic-ai-factory.com](https://magic-ai-factory.com) · **Oráculos:** [oracles.modelmarket.dev](https://oracles.modelmarket.dev) · **Política de versiones:** [`docs/sdk-version-policy.md`](../../docs/sdk-version-policy.md)

Esta es **la guía insignia de los SDKs**. Es técnica, con abundancia de tablas y orientada a ejemplos. Cada firma, header, puerto y dirección que aparece a continuación está tomado del código en vivo en `aimarket-sdks/`.

---

## 1. Qué es esto

El AIMarket Protocol v2 es un marketplace JSON/HTTP universal para capacidades de IA. Los vendedores publican capacidades detrás de hubs; los consumidores las descubren por **intent**, abren un **canal de pago prefinanciado**, **invocan** la capacidad (pagando por llamada desde el canal) y **liquidan** para recuperar lo que no gastaron. Las capacidades pueden ejecutarse dentro de un **TEE** (enclave de ejecución confiable), y el protocolo te permite verificarlo criptográficamente antes y después de una llamada. Los hubs **federan**, de modo que un hub puede enrutar una invocación hacia una capacidad alojada en otro.

Los cuatro SDKs:

| SDK | Paquete | Runtimes de destino |
|-----|---------|-----------------|
| **Dart** | `aimarket_agent` | Flutter desktop (macOS/Windows/Linux), servidores Dart |
| **TypeScript** | `@aimarket/agent` | Electron, servidores Node.js, aplicaciones web |
| **Rust** | `aimarket-agent` (crate) | Tauri, herramientas CLI nativas |
| **Python** | `aimarket-agent` (PyPI) | LangChain / agentes de servidor / CLI — *separado, sin estado, sin wallet* → [guía de Python](https://github.com/alexar76/aimarket-agent/blob/main/docs/en.md) |

URLs en vivo que vas a referenciar:

- **Hub de referencia (punto de entrada del consumidor):** `https://modelmarket.dev` — AIMarket Protocol v2. Desarrollo local: `http://localhost:9083`.
- **App / sitio de Factory:** `https://magic-ai-factory.com` — donde se construyen y publican las capacidades.
- **Portal de oráculos:** `https://oracles.modelmarket.dev` — diecisiete oráculos de matemática verificable en vivo, todos accesibles como capacidades.

Usa `https://modelmarket.dev` como el `hubUrl` canónico en tu propio código, salvo que apuntes a un hub autoalojado o federado.

---

## 2. Elige tu SDK + instalación

| Runtime | Paquete | Instalación | Línea de versión |
|---------|---------|---------|--------------|
| **Flutter / Dart** | `aimarket_agent` | `dart pub add aimarket_agent` | **0.1.x** |
| **Electron / Node.js** | `@aimarket/agent` | `npm install @aimarket/agent` | **0.1.x** |
| **Tauri / Rust** | `aimarket-agent` (crate) | `cargo add aimarket-agent` | **0.1.x** |
| **Servidor / LangChain / CLI** | `aimarket-agent` (PyPI) | `pip install aimarket-agent` → [guía de Python](https://github.com/alexar76/aimarket-agent/blob/main/docs/en.md) | **2.1.x** |

> **Dos líneas de versión, a propósito.** Dart / TypeScript / Rust son la familia multiplataforma sincronizada en **0.1.x**; el agente de Python es un paquete más antiguo y separado en **2.1.x** en PyPI. Ambos apuntan a AIMarket Protocol v2. Consulta [`docs/sdk-version-policy.md`](../../docs/sdk-version-policy.md) — no "arregles" el desajuste.

### Instalación desde el registro (recomendada)

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

### Instalación monorepo / git (desarrollo local)

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

## 3. Autenticación — la verdad sobre Ed25519

**La clave de wallet con la que firman estos SDKs es una semilla Ed25519, no una clave privada de Ethereum/secp256k1.** Esto es lo más importante que debes entender bien.

- `walletKey` es una **cadena hex de 64 caracteres = una semilla Ed25519 de 32 bytes**. El firmante la convierte en un par de claves Ed25519.
- Si la cadena **no** es hex de 64 caracteres, se le aplica SHA-256 para obtener una semilla de 32 bytes (un fallback de desarrollo — nunca lo uses en producción).
- Cada solicitud de **invoke** se firma sobre la cadena canónica `channel:<id>|capability:<id>|affiliate:<affil>`, produciendo una firma de la forma `ed25519:<base64>`, enviada como el header `X-Market-Signature`.

El firmante de Dart (`dart/lib/src/signer.dart`) deja explícitos los tres puntos:

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

### Cargar la clave de forma segura (nunca la incrustes en el código)

Obtén la semilla del keychain / almacenamiento seguro de la plataforma en tiempo de ejecución. No la incluyas en el control de versiones, no la integres en tu binario.

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

### La clave secundaria opcional (EIP-712 / secp256k1)

Existe una **segunda clave opcional** usada únicamente para **débitos de canal on-chain** contra `AIMarketEscrow.debitChannel` — una firma EIP-712 sobre una clave secp256k1 (`eip712:0x<r><s><v>`). En Dart es `MarketSigner(ethereumPrivateKeyHex: ...)`, consumida por `signDebitAuthorization(...)`. **La mayoría de los consumidores nunca la necesitan** — el flujo de channel/invoke/settle se ejecuta enteramente sobre la semilla Ed25519. Recurre a ella solo si liquidas canales directamente on-chain.

> **No** llames a `walletKey` una "clave privada EVM/Ethereum". Es una semilla Ed25519. La clave secp256k1/EIP-712 es una entrada distinta y opcional.

---

## 4. El ciclo de valor de 5 fases

Cada SDK implementa las mismas cinco fases contra los mismos endpoints del hub.

| # | Fase | Endpoint del hub v2 | Método (se muestra Dart) | Qué hace |
|---|-------|-----------------|---------------------|--------------|
| 1 | **Discovery** | `GET /.well-known/ai-market.json` → `GET /ai-market/v2/search` | `discover(...)` | encuentra capacidades que coincidan con un intent + presupuesto |
| 2 | **Channel** | `POST /ai-market/v2/channel/open` | `openChannel(depositUsd, token, chain)` | abre un canal de pago prefinanciado |
| 3 | **Invoke** | `POST /ai-market/v2/invoke` (headers `X-Payment-Channel`, `X-Market-Signature`) | `invoke(...)` | llama a una capacidad, paga desde el canal |
| 4 | **Settle** | `POST /ai-market/v2/channel/close` | `closeChannel(channelId)` | cierra el canal, reembolsa el saldo no gastado |
| 5 | **Verify** | *(local)* | `verifyTeeAttestation(...)` / `verifyTeeReceipt(...)` | atestación TEE antes de enviar + receipt después |

`runOnce(...)` es el método de conveniencia que ejecuta las cinco fases y devuelve un `BillOfMaterials`.

### Superficie de métodos (por lenguaje)

| Fase | Dart | TypeScript | Rust |
|-------|------|------------|------|
| Discover | `discover({intent, budget?, limit=5, category?})` | `discover({ intent, budget?, limit?, category? })` | `discover(intent, budget, limit, category)` |
| Channel | `openChannel(depositUsd, {token='USDT', chain='base'})` | `openChannel(depositUsd, token='USDT', chain='base')` | `open_channel(deposit_usd, token, chain)` |
| Invoke | `invoke({capabilityId, input, channelId, sourceHub?, productId?, verifyTee=true, attestation?})` | `invoke({ capabilityId, input, channelId, productId?, sourceHub?, verifyTee?, attestation? })` | `invoke(capability_id, input, channel_id, product_id, source_hub)` |
| Settle | `closeChannel(channelId)` | `closeChannel(channelId)` | `close_channel(channel_id)` |
| Ciclo completo | `runOnce({intent, input, depositUsd=5.00, category?})` | `runOnce({ intent, input, depositUsd?, category? })` | `run_once(intent, input, deposit_usd, category)` |

> **El nombrado de los campos del resultado difiere según el idiom de cada lenguaje.** Dart usa camelCase (`result.priceUsd`, `result.teeVerified`, `result.teeReceipt`); TypeScript y Rust usan snake_case (`result.price_usd`, `result.tee_verified`, `result.tee_receipt`). El JSON de transporte es snake_case en todo momento. Pasos del plan: Dart expone `step.capability.id`; TS/Rust exponen `step.capability.capability_id`.

### De una sola pasada: `runOnce(...)`

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

### Ciclo manual: Discovery → Channel → Invoke → Settle

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

> **Valores por defecto que conviene conocer:** `openChannel` usa por defecto `token='USDT'`, `chain='base'`. `runOnce` usa por defecto `depositUsd=5.00`. El `timeout` por solicitud es de 30s y `maxRetries` es 3 (solo se reintentan los errores de red/timeout, con backoff de 1s/2s/4s; los 402/403 y los errores de protocolo se propagan de inmediato). El afiliado por defecto es propio de cada lenguaje: `aimarket-sdk-dart`, `aimarket-sdk-ts`, `aimarket-sdk-rust`.

---

## 5. Verificación TEE

Si una capacidad se ejecuta en un enclave confiable, puedes demostrarlo — **antes** de enviar datos, y **después** de recibir la salida. El patrón es: registrar el hash de código esperado, verificar la atestación, abortar si falla, y luego verificar el receipt.

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

Dos formas complementarias de verificar, más una comprobación previa integrada:

- **`verifyTeeAttestation(attestation, capabilityId)`** — antes de enviar. Comprueba el hash de código y la firma del enclave contra tu conjunto de confianza. Devuelve `false` si no se puede confiar en el enclave.
- **`verifyTeeReceipt(receipt, sentInput, receivedOutput)`** *(Dart / TypeScript)* — después de recibir. Demuestra que la salida se produjo en el enclave atestado a partir del input exacto que enviaste.
- **Comprobación previa integrada:** cuando pasas una `attestation` a `invoke(..., verifyTee: true)`, el SDK la verifica **antes de transmitir cualquier input** y **falla en modo cerrado** — una atestación inválida aborta la llamada (Dart/TS lanzan la excepción de seguridad) para que los datos sensibles nunca lleguen a un enclave no verificado.

> **Por qué importa:** sin verificación, confías en la palabra del vendedor de que el código que crees que se ejecuta realmente se ejecuta, en un enclave real. Verifica la atestación antes de filtrar input; verifica el receipt antes de confiar en la salida.

---

## 6. Federación — llamar a una capacidad en otro hub

Los hubs federan. Para invocar una capacidad que reside en otro hub, pasa `sourceHub` (y normalmente el `productId`). Tu canal y tu firma permanecen con tu hub de origen; este enruta la llamada.

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

`runOnce(...)` ya hace esto automáticamente: reenvía el `capability.sourceHub` y el `capability.productId` de cada paso del plan al invoke, de modo que los resultados federados se enrutan correctamente sin trabajo adicional.

---

## 7. Manejo de errores

Los SDKs mapean los códigos de estado HTTP a errores tipados con rutas de recuperación claras.

| Error | Código | Significado | Recuperación |
|-------|------|---------|----------|
| Discovery failed | 4xx/5xx | hub inalcanzable / intent inválido | reintentar, revisar la URL del hub |
| Channels unavailable | 404 | el hub no tiene plugin de canales | usar otro hub, o `runOnce` |
| Payment required | 402 | canal agotado o expirado | abrir un canal mayor, reintentar |
| Safety blocked | 403 | el input activó un control de seguridad | revisar / depurar el input (PII), reintentar |
| TEE not verified | 200 | falló la atestación/receipt | verificar manualmente, tratar la salida como no confiable |

### Tipos de excepción

- **Dart:** `AimarketException` (base) con las subclases `AimarketNetworkException`, `AimarketPaymentException` (402), `AimarketSafetyException` (403, expone `.reason`). Inspecciona `.message` y `.statusCode`.
- **TypeScript:** la misma jerarquía — `AimarketException`, `AimarketNetworkException`, `AimarketPaymentException`, `AimarketSafetyException`.
- **Rust:** el enum `AimarketError` — `Network(..)`, `Payment(..)`, `Safety(..)`, `Protocol(..)`.

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

Ten en cuenta que un código no-2xx que no sea ni 402 ni 403 **no** lanza una excepción desde `invoke` — devuelve un `InvokeResult` con `success: false` y `error` definido, así que revisa siempre `result.success` incluso cuando no se lance ninguna excepción.

---

## 8. Avanzado

### Reutilización de canales + patrón de wrapper de servicio

Abre un canal una vez y reutilízalo en muchas invocaciones. El SDK ya cachea canales con clave `deposit:token:chain` y reutiliza uno mientras esté **sin expirar y tenga > 50% de saldo restante**; por debajo de eso abre uno nuevo de forma transparente. En una aplicación de larga duración, envuelve el agente en un servicio que mantenga un canal hasta que su saldo caiga por debajo de un umbral, y entonces lo cierre + reabra — y siempre haz `dispose()` / cierra al apagar para que los fondos no gastados se reembolsen.

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

### Elimina el PII antes de las llamadas remotas

Las capacidades se ejecutan en el hub de otra persona. **Sanitiza del lado del cliente** — no confíes en que el hub o el vendedor lo depuren por ti. Elimina los identificadores directos (`email`, `phone`, nombres, `resume_text`, …) y aplica un hash de un solo sentido a cualquier dato que debas correlacionar (p. ej. `company` → `company_hash`).

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

### Pipelines multicapacidad

Para flujos de trabajo que componen varias capacidades en un DAG sobre un único canal compartido — con estimación de costo por paso y fan-out — consulta el [ejemplo de integración de capability-composer](https://github.com/alexar76/aimarket-desktop/blob/main/capability-composer/docs/sdk-integration.md). `invokeBatch(...)` ejecuta muchas capacidades sobre un solo canal (Dart/TS hacen fan-out de forma concurrente; Rust las ejecuta secuencialmente) y es el bloque de construcción de estos pipelines.

---

## 9. Versionado y soporte

Dart / TypeScript / Rust son la familia multiplataforma sincronizada en **0.1.x**; el agente de Python es un paquete separado en **2.1.x** en PyPI. Las versiones siguen al *paquete*, no al protocolo — los cuatro apuntan a AIMarket Protocol v2. Los vectores de prueba multilenguaje viven en `aimarket-sdks/test-vectors/`, y un guard de paridad del ecosistema mantiene los tres SDKs compilados en versiones y formas de modelo coincidentes en CI. Detalle completo: [`docs/sdk-version-policy.md`](../../docs/sdk-version-policy.md).

---

## 10. Documentación relacionada

- [README de aimarket-sdks](../README.md) — inicios rápidos para los tres SDKs compilados
- [README del SDK de Dart](../dart/README.md) · [README del SDK de TypeScript](../typescript/README.md) · [README del SDK de Rust](../rust/README.md)
- [Guía del SDK de Python](https://github.com/alexar76/aimarket-agent/blob/main/docs/en.md) — el consumidor sin estado y sin wallet
- [Política de versiones de los SDKs](../../docs/sdk-version-policy.md)
- [Oráculos — capacidades de matemática verificable](https://github.com/alexar76/oracles/blob/main/docs/en.md)
- Ejemplos de integración: [interview-prep-coach](https://github.com/alexar76/aimarket-desktop/blob/main/interview-prep-coach/docs/sdk-integration.md) · [cold-outreach-coach](https://github.com/alexar76/aimarket-desktop/blob/main/cold-outreach-coach/docs/sdk-integration.md) · [capability-composer](https://github.com/alexar76/aimarket-desktop/blob/main/capability-composer/docs/sdk-integration.md)
- En vivo: [modelmarket.dev](https://modelmarket.dev) · [magic-ai-factory.com](https://magic-ai-factory.com) · [oracles.modelmarket.dev](https://oracles.modelmarket.dev)

---

🇬🇧 [English](en.md) · 🇷🇺 [Русский](ru.md) · 🇪🇸 [Español](es.md)
