# NEAR HACKATHON (1–14 листопада 2026) — нотатки

Зібрано 5 листопада 2026. Джерела: https://nearhackathon.com/ і https://nearhackathon.com/HACKATHON.md
(правила можуть змінюватись — перечитати HACKATHON.md перед сабмітом).

## Ключові факти

| Що | Значення |
|---|---|
| Старт / відкриття сабміту | 1 листопада 2026, 00:00 UTC |
| Дедлайн розробки і сабміту | **14 листопада 2026, 23:59 UTC** |
| Суддівство / результати | 16–18 листопада 2026 |
| Код рахується | з анонсу 19 жовтня 2026 (старі коміти — baseline) |
| Призовий фонд | $250 000, виплата в NEAR на mainnet-акаунт, протягом 30 днів |
| Гран-прі | $55 000 (один проєкт) |
| Кожен із 13 треків | 1 місце $10 000, 2 місце $3 000, 3 місце $2 000 |
| Реєстрація | не потрібна; одна форма наприкінці |

## Треки (id для форми)

defi, thesis, **privacy**, business, swarms, security, agents, realworld, games, nfts, memecoins, social, open.
Один проєкт можна подати одразу в кілька треків (multi-select). Хінти треків — побажання, не вимоги.

## Сабміт (одна спроба на репозиторій, редагувати не можна)

- Публічний репо (можна тримати приватним до дедлайну, але о 23:59 UTC 14.11 має бути публічним), файл LICENSE (MIT ок).
- Публічний пост в X: «built at NEAR HACKATHON», тег @NEARProtocol і #NEARHackathon, відео або скріншоти. Локальний демо ок, якщо є відео + відтворюваний README.
- Форма: назва, тімлід, email (макс. 2 сабміти на email), розмір команди, треки, NEAR mainnet-акаунт для призу, лінк на репо, лінк на пост, опис 20–2000 символів.
- Поле `website` — спам-пастка: залишити порожнім.
- Якщо проєкт існував раніше: окрема гілка або теги pre-hackathon / hackathon-submission + секція в README з лінком на diff.
- Бібліотеки й темплейти дозволені, якщо перелічені в README.
- Testnet-only проєкти допускаються в усіх треках. NEAR Intents не має testnet — mainnet з копійчаними сумами і `dry: true` котируваннями.

## Суддівство

- Спершу AI-скринінг (Claude Code і Codex): перевіряють, що проєкт запускається і робить те, що написано в README. README писати для машини: нумеровані кроки «зроби X, побачиш Y».
- Судді-люди: Illia Polosukhin (NEAR), Eddy Lazzarin (a16z crypto), Mert Mumtaz (Helius), Avichal Garg (Electric Capital), Kain Warwick (Synthetix/Infinex), Haseeb Qureshi (Dragonfly), Stepan Gershuni (cyber•Fund).
- Критерії минулих хакатонів: technical implementation, design, impact, originality, sustainability.

## Трек Privacy & Private AI (id: privacy)

«Explore confidential computation, private finance and AI that protects sensitive data.» Lean on: Private AI, TEE, NEAR AI Cloud.
Стартові лінки з офіційного промпта (tracks/03-privacy.md у near-hackathon-prompt-examples/):
- https://docs.near.ai/cloud/quickstart
- https://docs.near.ai/cloud/verification
- https://github.com/near-examples/nearai-cloud-verification-example
- https://docs.agentsonintents.com/integration/agent-api/overview

## Що реально є на NEAR для privacy (стан на листопад 2026)

1. **Confidential Intents** — приватний шард NEAR (permissioned-валідатори, TEE-міст у mainnet). Доступ лише через 1Click Swap API; з 27.10.2026 свопи 1Click confidential за замовчуванням. Публічних доків для деплою власних контрактів на приватний шард немає → це рейка для розрахунків, не платформа для білду.
2. **NEAR AI Cloud** — LLM-інференс у TEE (Intel TDX + NVIDIA GPU attestation), OpenAI-сумісний API `https://cloud-api.near.ai/v1`, ендпойнт `GET /v1/attestation/report?nonce=...`, підписи відповідей `GET /v1/signature/<id>`. $10 кредитів через https://nearbuilders.org/join. Тільки інференс, не загальні обчислення.
3. **OutLayer** (FastNEAR) — verifiable off-chain compute: Rust → WASM (WASI), виконується в Intel TDX, attestation покриває хеш бінарника і хеш інпуту; виклик із контракту через yield/resume або HTTPS. Є encrypted secrets, VRF, key derivation. Докs: https://outlayer.fastnear.com/docs
4. **Shade Agents** — офіційно deprecated з 19.04.2026 (доки переїхали в GitHub NearDeFi/shade-agent-framework; працює тільки на dstack/Phala). Будувати на цьому ризиковано.
5. **ZK on-chain** — near-sdk 5.30 має host functions `alt_bn128_g1_multiexp`, `alt_bn128_g1_sum`, `alt_bn128_pairing_check` (BN254), а також `bls12381_*`, `ed25519_verify`, `p256_verify`, keccak/sha256. Groth16-пруф верифікується за ≈35 Tgas (виміряно проєктом nymira на sandbox, nearcore 2.13.4/2.14). Хост сам абортить невалідні/не-subgroup точки G2. Референс-код: https://github.com/zeropoolnetwork/near-groth16-verifier (2022, старі форки) і https://github.com/nymiralabs/nymira (активний, nov 2026, near-sdk 5.29.1).
6. **Конкурент у треку**: nymira — приватний vault передплачених API-кредитів (Poseidon Merkle-дерево глибини 32, Groth16, testnet USDC). Не DEX.

## Penumbra як референс

- ZSwap: shielded pool + batch swaps раз на блок для кожної пари; виконання проти concentrated liquidity; аутпути забираються SwapClaim-транзакцією у shielded pool.
- Penumbra V1: користувач ПУБЛІЧНО спалює інпут (з shielded pool, тобто unlinkable), ПРИВАТНО мінтить аутпут. Sealed-bid (threshold encryption) — ще не запущено.
- Приватність батчу слабка, якщо в батчі 1 своп. Доки: https://protocol.penumbra.zone/main/dex/swap.html

## Варіанти проєкту «Penumbra на NEAR»

A. **ZK shielded pool + batch swaps (Penumbra V1 на NEAR)** — найближче до оригіналу, найбільший ризик по часу.
   Контракт: Poseidon Merkle-дерево нот, nullifier set, Groth16-верифікатор (alt_bn128), batch-executor (один нет-своп на пару за епоху проти Rhea/Ref на testnet або 1Click на mainnet, uniform clearing price).
   Схеми (circom): spend/output; swap_claim. Prover у браузері (snarkjs). Треки: privacy + defi + open.
B. **Sealed-bid batch auction у TEE (Penumbra «V2» через TEE)** — ордери шифруються на ключ enclave (OutLayer TDX-WASM або dstack), enclave раз на епоху рахує clearing price, публікує агрегати + attestation; контракт на NEAR: ескроу + перевірка attestation; розрахунок через Confidential Intents. Менше криптографії, більше інфраструктури. Найкраще лягає на хінти треку (TEE).
C. **Shielded pool + Confidential Intents (мінімальний ZK)** — приватні баланси/перекази NEP-141 через нотки/нуліфаєри; вихід із пулу напряму в confidential 1Click-своп. Без батч-свопів. Fallback для A.

## Старт роботи

- `npx skills add near/agent-skills` (скіли near-smart-contracts, near-intents, near-ai-cloud, near-api-js, near-kit, near-dapp).
- Доки через https://docs.near.org/llms.txt і https://docs.near-intents.org/llms.txt; адреси контрактів/ендпойнти не вгадувати.
- Testnet спочатку; ключі в .env; README з першого дня.
- Спільнота: https://discord.gg/nearprotocol , https://t.me/nearbuilderschat

## BLS12-381 на NEAR (NEP-488), перевірено 5.11.2026

- 9 host functions: bls12381_p1_sum, p2_sum, g1_multiexp, g2_multiexp, map_fp_to_g1, map_fp2_to_g2, pairing_check, p1_decompress, p2_decompress. Для zkSNARK потрібні p1_sum, g1_multiexp, pairing_check.
- Кодування Zcash-стилю (НЕ EIP-2537): Fp 48 байт big-endian, Fp2 = c1 || c0, скаляр 32 байти little-endian (може перевищувати порядок групи), G1 uncompressed 96 байт, G2 192 байти, нескінченність = біт 0x40 у першому байті. Sum-функції: 1 байт знака + точка. Multiexp: точка, потім скаляр. Pairing: 288 байт на пару (G1 || G2).
- Коди повернення: 0 ок, 1 невалідне кодування/не на кривій/не в підгрупі, 2 (тільки pairing) валідний інпут, але добуток не одиниця. multiexp і pairing перевіряють підгрупу; p1_sum/p2_sum ні.
- Типізований модуль `near_sdk::bls12381` (типи G1, G2, Fp, Fp2, Scalar, Sign; pairing_check -> Result<bool, Error>) з near-sdk 5.30.0-rc.1 (PR #1613, вересень 2026). Байтові `env::bls12381_*` повертають None при відхиленні хостом.
- Gas: NEP не публікує таблицю (лише формула A + B*k). Треба міряти в sandbox. Баг із gas для register-input виправлено в nearcore PR #16753 (protocol 89).
- Для порівняння: alt_bn128 (BN254) Groth16 ≈ 35 Tgas на пруф, виміряно nymira на nearcore 2.13.4 / 2.14.

## Рішення A vs B (5.11.2026)

Рекомендація: A як ядро (семантика Penumbra V1: публічний burn інпуту з shielded pool, приватний mint аутпуту, uniform clearing price на батч), B (sealed-bid дельт через TEE) як stretch-шар зверху. Spike дня 1: gas Groth16-верифікації (a) alt_bn128, (b) bls12381, (c) чистий WASM над BLS12-377 (якщо влізе, можна перевикористати крейти Penumbra).

## Spike дня 1: gas Groth16-верифікації на NEAR (sandbox nearcore 2.14.0, 5.11.2026)

Toy-схема, 4 публічні входи. `receipt` = gas function-call receipt (включно з ~1.68 Tgas базової вартості виклику).

| Шлях | receipt Tgas | Нетто Tgas |
|---|---|---|
| noop (база) | 1.68 | 0 |
| BN254 через alt_bn128 host functions | 34.20 | ~32.5 |
| **BLS12-381 через NEP-488 host functions** | **17.15** | **~15.5** |
| BN254 чистий wasm (ark-groth16) | 56.52 | ~54.8 |
| BLS12-377 чистий wasm (ark-groth16, крива Penumbra) | 129.91 | ~128.2 |
| Poseidon(2) над BN254 Fr у wasm (light-poseidon), маргінально | | ~1.2 на хеш |

Усі валідні пруфи пройшли, підмінений публічний вхід дав `false` на обох host-шляхах (кодування G2 c0||c1 для alt_bn128 і c1||c0 для NEP-488 підтверджене).

Висновки:
- BLS12-381 host functions удвічі дешевші за alt_bn128 і працюють на nearcore 2.14 (testnet на protocol 88 їх має).
- Чистий WASM над BLS12-377 (130 Tgas) влазить у ліміт 300 Tgas лише для одного пруфа на транзакцію. Транзакційна модель Penumbra (spend + output + swap в одному tx, кожен зі своїм пруфом) у ліміт не влазить, тож перевикористати реальні схеми Penumbra як є не вийде. Власні join-split/swap_claim схеми, один пруф на дію.
- Вставка в Poseidon-дерево глибини 20 ≈ 24 Tgas; join-split (пруф + 2 вставки) ≈ 70 Tgas. Запас до 300 Tgas великий.
- Контракт spike: 656 KB wasm з arkworks усередині (для host-шляхів arkworks у контракті не потрібен).

Poseidon-крейти Penumbra (poseidon-permutation 1.1, poseidon-paramgen 0.4, poseidon377 1.2, decaf377 0.10) сидять на arkworks 0.4 (2023–2024). Spike написаний на arkworks 0.6 (gr1cs, ark-crypto-primitives 0.6 має sponge Poseidon + constraints + merkle_tree). Вибір стеку: або 0.6 + ark-crypto-primitives (працює вже зараз), або пін 0.4 + Penumbra-крейти для Poseidon (знайомий інструментарій). Рішення за Валентином.

## День 2 (5.11.2026): shielded pool працює наскрізь

Зроблено: `crates/antumbra-core` (Poseidon над BLS12-381 Fr через ark-crypto-primitives, ноти/нуліфаєри, дерево глибини 20 з frontier для контракту, шифрування нот X25519+ChaCha20-Poly1305, R1CS-гаджети), `crates/antumbra-circuits` (join-split 2-in-2-out, 17 312 констрейнтів, детермінований setup, експорт VK у NEP-488 layout), `contracts/pool` (deposit через ft_on_transfer, joinsplit для transfer/withdraw, view-методи для скану), `contracts/test-ft`, `tests/e2e` (sandbox: deposit → transfer → scan → withdraw → два негативні кейси). Усе зелене.

Виміряно в sandbox: Groth16 verify 17 Tgas; Poseidon(2) у wasm ≈ 1.5 Tgas на хеш (ark sponge); joinsplit transfer 83 Tgas, withdraw 85 (+ft_transfer), deposit 38 total. Вбудовані константи Poseidon і нулі дерева (gen-consts) зекономили ~50 Tgas на виклик (генерація LFSR + 20 хешів нулів при кожному виклику).

Гочі, які коштували часу:
- **`--cfg near` обов'язковий для wasm-збірки near-sdk 5.x без cargo-near.** Без нього near-sdk-env іде в «local»-гілку: `panic_str` робить `panic!()` всередині panic-hook → подвійна паніка → abort → «WebAssembly trap: unreachable» замість повідомлення. Прапор у `.cargo/config.toml`.
- Dummy-інпут join-split із детермінованими sk/rseed дає той самий нуліфаєр у кожній транзакції; контракт його записує, друга транзакція падає як double-spend. Dummy тепер з ентропією на транзакцію.
- near-workspaces дає викликам 10 Tgas за замовчуванням; `Pool::new` рахує дерево → потрібен `max_gas()`.
- Застарілий макрос `impl_fungible_token_storage!` у near-contract-standards 5.30 не компілюється; `storage_deposit` реалізовано вручну.
- arkworks 0.6: `ark_relations::gr1cs`, `GR1CSVar` замість `R1CSVar`, `FieldBasedCryptographicSponge` для `squeeze_native_field_elements`, `ark-snark` без фічі `std`.

План дня 3: swap-дія (join-split з ext_hash = swap commitment), епохи з CPMM і BatchSwapOutputData, схема swap_claim, e2e на повний цикл свопу.
