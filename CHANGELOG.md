## [1.9.0-stacks-signers.7](https://github.com/hirosystems/chainhook/compare/v1.9.0-stacks-signers.6...v1.9.0-stacks-signers.7) (2024-11-07)


### Bug Fixes

* propagate error when trying to store peer info ([1bc949a](https://github.com/hirosystems/chainhook/commit/1bc949a6ce17ab67c2c1af2d21fe330461689f49))

## [1.9.0-stacks-signers.6](https://github.com/hirosystems/chainhook/compare/v1.9.0-stacks-signers.5...v1.9.0-stacks-signers.6) (2024-10-31)


### Bug Fixes

* read stacks-core http event POST payloads for ignored events ([#673](https://github.com/hirosystems/chainhook/issues/673)) ([a01470e](https://github.com/hirosystems/chainhook/commit/a01470e34876dcf67969063e1fc900ae947a0a54))

## [1.9.0-stacks-signers.5](https://github.com/hirosystems/chainhook/compare/v1.9.0-stacks-signers.4...v1.9.0-stacks-signers.5) (2024-10-25)


### Bug Fixes

* mock signature retrieval ([#671](https://github.com/hirosystems/chainhook/issues/671)) ([8bb59c1](https://github.com/hirosystems/chainhook/commit/8bb59c1b4727a37df47e08b7a31456bcb6528289))

## [1.9.0-stacks-signers.4](https://github.com/hirosystems/chainhook/compare/v1.9.0-stacks-signers.3...v1.9.0-stacks-signers.4) (2024-10-25)


### Bug Fixes

* bump stacks codec ([3c117b5](https://github.com/hirosystems/chainhook/commit/3c117b562d02b8b5379fd4b0f3187567d7f5289b))

## [1.9.0-stacks-signers.3](https://github.com/hirosystems/chainhook/compare/v1.9.0-stacks-signers.2...v1.9.0-stacks-signers.3) (2024-10-25)


### Features

* add mock signature pubkey ([#670](https://github.com/hirosystems/chainhook/issues/670)) ([c414d79](https://github.com/hirosystems/chainhook/commit/c414d793e9a25c1b541914fee256d98d53bc08a3))

## [1.9.0-stacks-signers.2](https://github.com/hirosystems/chainhook/compare/v1.9.0-stacks-signers.1...v1.9.0-stacks-signers.2) (2024-10-25)


### Features

* add mock signer message support ([#669](https://github.com/hirosystems/chainhook/issues/669)) ([ad5fd54](https://github.com/hirosystems/chainhook/commit/ad5fd54b3d1d8f638fa44a531bca71306fbb8c6f))

## [1.9.0-stacks-signers.1](https://github.com/hirosystems/chainhook/compare/v1.8.0...v1.9.0-stacks-signers.1) (2024-10-25)


### Features

* add various new nakamoto block fields to `/new_block` ingestion and `StacksPayload` ([#659](https://github.com/hirosystems/chainhook/issues/659)) ([f48cda5](https://github.com/hirosystems/chainhook/commit/f48cda533dfc58bb630737fb29cf1bc9c966a638))
* include recovered signer pubkeys in new block payload ([#662](https://github.com/hirosystems/chainhook/issues/662)) ([b5ad4ba](https://github.com/hirosystems/chainhook/commit/b5ad4ba11dd72722bb6cbe936ec29411cde9a606))
* include signer messages in Stacks predicate payloads ([#656](https://github.com/hirosystems/chainhook/issues/656)) ([aee14bc](https://github.com/hirosystems/chainhook/commit/aee14bc693573f403f8a6d8eafe7b30d2ca76b54))
* parse `/stackerdb_chunks` Stacks node event ([#653](https://github.com/hirosystems/chainhook/issues/653)) ([e44d84a](https://github.com/hirosystems/chainhook/commit/e44d84a0d739921a5a3ccae6e9643bdb85005f71))
* store signer messages in local sqlite database ([#664](https://github.com/hirosystems/chainhook/issues/664)) ([d12acd9](https://github.com/hirosystems/chainhook/commit/d12acd9c99ce0d0721dfcb853e5cb236e45925b7))
* support tenure-height in block messages ([#661](https://github.com/hirosystems/chainhook/issues/661)) ([99e5a61](https://github.com/hirosystems/chainhook/commit/99e5a6119fb8e87e1e14d789fdb866eae230e8e8))
* **ts-client:** add persisted predicates and periodic health check ([#658](https://github.com/hirosystems/chainhook/issues/658)) ([535226a](https://github.com/hirosystems/chainhook/commit/535226a80480585aabfcf89d68b21d2ada1c50c1))
* **ts-client:** add signer messages to stacks payloads ([#657](https://github.com/hirosystems/chainhook/issues/657)) ([ff66bb2](https://github.com/hirosystems/chainhook/commit/ff66bb2eab771c16fead154607a280664eb6be4e))


### Bug Fixes

* add a configurable max timeout for outgoing predicate payload requests ([#642](https://github.com/hirosystems/chainhook/issues/642)) ([6c1dfa9](https://github.com/hirosystems/chainhook/commit/6c1dfa9f741041d0fac32e2a89c04a71b4a485cd)), closes [#643](https://github.com/hirosystems/chainhook/issues/643)
* increase stacks event server body size limit to 500MB ([#660](https://github.com/hirosystems/chainhook/issues/660)) ([21234c9](https://github.com/hirosystems/chainhook/commit/21234c978d4c49ea5526c2cf1f74d42a645a68c8))
* return 500 status code to Stacks node when event ingestion fails ([#648](https://github.com/hirosystems/chainhook/issues/648)) ([63c753c](https://github.com/hirosystems/chainhook/commit/63c753c1a1b043e271d1ebca1f90f154030a89e8)), closes [#646](https://github.com/hirosystems/chainhook/issues/646)
* signer pubkey calculation ([#665](https://github.com/hirosystems/chainhook/issues/665)) ([cc93873](https://github.com/hirosystems/chainhook/commit/cc93873dacc88e5df97d1006394b99744e63009b))
* update SignerMessage deserializing ([#663](https://github.com/hirosystems/chainhook/issues/663)) ([71364c1](https://github.com/hirosystems/chainhook/commit/71364c15a88a06d5c71b3f3d3c88c489ff4733d1))
* validate ts client predicates before registering ([#639](https://github.com/hirosystems/chainhook/issues/639)) ([2d08f72](https://github.com/hirosystems/chainhook/commit/2d08f72e44446e9c311f8a64f992e103f17a4ae7))

## [1.8.0](https://github.com/hirosystems/chainhook/compare/v1.7.0...v1.8.0) (2024-08-12)

### New Features
* PoxConfig improvements (#616)

### Bug Fixes

* optimize memory for stacks tsv import into rocksdb (#634)

## [1.7.0](https://github.com/hirosystems/chainhook/compare/v1.6.2...v1.7.0) (2024-07-05)

### New Features
* predicate validation (#611)
* improve chainhook-sdk interface (#608)

### Bug Fixes

* allow aborting a predicate scan (#601)

### CI
* add snapcraft.yaml (#607) - releases are now available on the [snap store](https://snapcraft.io/chainhook)!
* winget pkg version bump (#606) - releases are now available on winget!

## [1.6.2](https://github.com/hirosystems/chainhook/compare/v1.6.1...v1.6.2) (2024-06-06)

### Bug Fixes

* use `stacks_node_rpc_url` from config (#579)
* print to console for unknown cli commands (#595)

### CI
* revamp ci (#594) - This PR adds builds to our [Releases](https://github.com/hirosystems/chainhook/releases) page for Windows, Linux, and MacOS!

  
## [1.6.1](https://github.com/hirosystems/chainhook/compare/v1.6.0...v1.6.1) (2024-05-15)

### Bug Fixes

* serialize brc-20 data ([#585](https://github.com/hirosystems/chainhook/issues/585)) ([9011a0c](https://github.com/hirosystems/chainhook/commit/9011a0ce49d9d38a6d7a6776d7f37b709a355386))


## [1.6.0](https://github.com/hirosystems/chainhook/compare/v1.5.1...v1.6.0) (2024-05-09)


### Features

* add Bitcoin transaction index to typescript client ([#568](https://github.com/hirosystems/chainhook/issues/568)) ([6f7eba4](https://github.com/hirosystems/chainhook/commit/6f7eba4c60d96dc088c08708cc5af2381ee62012))


### Bug Fixes

* add stacks event position to ts client ([#575](https://github.com/hirosystems/chainhook/issues/575)) ([3c48eeb](https://github.com/hirosystems/chainhook/commit/3c48eeb8adf0b6ef7998702f1d082c960f00f950))
* add starting stacks height for prometheus metrics ([#567](https://github.com/hirosystems/chainhook/issues/567)) ([6a8c086](https://github.com/hirosystems/chainhook/commit/6a8c0869278aad54f5100216ceb5b2d8f98ad002))
* make bitcoin payload serialization deserializable ([#569](https://github.com/hirosystems/chainhook/issues/569)) ([5f20a86](https://github.com/hirosystems/chainhook/commit/5f20a869acbd057d855b9601a4fb1072e75ab4c4))
* set `Interrupted` status for streaming predicates that fail ([#574](https://github.com/hirosystems/chainhook/issues/574)) ([11bde53](https://github.com/hirosystems/chainhook/commit/11bde5344b1b42bd7561e632e36265a9698fc095)), closes [#523](https://github.com/hirosystems/chainhook/issues/523)
* shut down observer on bitcoin block download failure ([#573](https://github.com/hirosystems/chainhook/issues/573)) ([f3530b7](https://github.com/hirosystems/chainhook/commit/f3530b74d9d25c111338d06085e2dae6fe527932)), closes [#572](https://github.com/hirosystems/chainhook/issues/572)

## [1.5.1](https://github.com/hirosystems/chainhook/compare/v1.5.0...v1.5.1) (2024-04-12)


### Bug Fixes

* correctly track bitcoin block append metrics; add additional metrics ([#563](https://github.com/hirosystems/chainhook/issues/563)) ([f85ac9e](https://github.com/hirosystems/chainhook/commit/f85ac9e56718264c48a4af33e7d2514319022a2d)), closes [#562](https://github.com/hirosystems/chainhook/issues/562)

## [1.5.0](https://github.com/hirosystems/chainhook/compare/v1.4.1...v1.5.0) (2024-04-06)


### Features

* add brc-20 schemas for ordhook ([#551](https://github.com/hirosystems/chainhook/issues/551)) ([1e25a8f](https://github.com/hirosystems/chainhook/commit/1e25a8f30664d814f510292665c6de17bc5e5c20))
* detect http / rpc errors as early as possible ([e515116](https://github.com/hirosystems/chainhook/commit/e5151168d75b2568a4a396e675b49bc917edad50))
* rm blocks ([f35498d](https://github.com/hirosystems/chainhook/commit/f35498d7f5efc3d9a92bf6e88f7f79df1ff879a0))
* use stacks.rocksdb for predicate scan ([#514](https://github.com/hirosystems/chainhook/issues/514)) ([0baae10](https://github.com/hirosystems/chainhook/commit/0baae107e65b329751a7c41ca738948ba65b6d40)), closes [#513](https://github.com/hirosystems/chainhook/issues/513) [#485](https://github.com/hirosystems/chainhook/issues/485)


### Bug Fixes

* build error ([03b3938](https://github.com/hirosystems/chainhook/commit/03b3938245232be00aa47d9464c5ed215942fc7d))
* log errors on block download failure; implement max retries ([#503](https://github.com/hirosystems/chainhook/issues/503)) ([3bfb0e1](https://github.com/hirosystems/chainhook/commit/3bfb0e1ae4c36ddddb424f928bbdd6c6121ef7ab))
* **metrics:** update latest ingested block on reorg ([#515](https://github.com/hirosystems/chainhook/issues/515)) ([823713a](https://github.com/hirosystems/chainhook/commit/823713aa8b850324b10ef94e6f104e8fec711eeb))
* order and filter blocks used to seed forking block pool ([#534](https://github.com/hirosystems/chainhook/issues/534)) ([a2865b7](https://github.com/hirosystems/chainhook/commit/a2865b7611e592f4c329fa6404da3fbcb662161f))
* revisit 7+ blocks reorg handling ([#553](https://github.com/hirosystems/chainhook/issues/553)) ([184fd07](https://github.com/hirosystems/chainhook/commit/184fd07f7767a25c2c3be20a474b972d773269e4))
* seed forking handler with unconfirmed blocks to improve startup stability ([#505](https://github.com/hirosystems/chainhook/issues/505)) ([b77aca2](https://github.com/hirosystems/chainhook/commit/b77aca2b8cd320491a67d275f97369c7547c61c0)), closes [#487](https://github.com/hirosystems/chainhook/issues/487)
* skip db consolidation if no new dataset was downloaded ([#513](https://github.com/hirosystems/chainhook/issues/513)) ([b1469a6](https://github.com/hirosystems/chainhook/commit/b1469a6753bf429ec54a3ffddfac49ddf96027eb))
* update scan status for non-triggering predicates ([#511](https://github.com/hirosystems/chainhook/issues/511)) ([32cdfee](https://github.com/hirosystems/chainhook/commit/32cdfeea66bb7b223e436a94fe4058056c05e3d2)), closes [#498](https://github.com/hirosystems/chainhook/issues/498)

## [1.4.1](https://github.com/hirosystems/chainhook/compare/v1.4.0...v1.4.1) (2024-04-02)


### Bug Fixes

* limit scope of stacks_rw_db usage to free lock ([#543](https://github.com/hirosystems/chainhook/issues/543)) ([0c287ca](https://github.com/hirosystems/chainhook/commit/0c287ca8ced9d5466fb0b3f4c8aac6aa7a914793))
* resolve dependencies ([#548](https://github.com/hirosystems/chainhook/issues/548)) ([86334f4](https://github.com/hirosystems/chainhook/commit/86334f4fc4a469d826c31b31cfb82e07eb8b5756))
* return unsupported kind for PoisonMicroblock ([#545](https://github.com/hirosystems/chainhook/issues/545)) ([123ec9d](https://github.com/hirosystems/chainhook/commit/123ec9de0e74504e616fb50deb323cb5ca138fda))

## [1.4.0](https://github.com/hirosystems/chainhook/compare/v1.3.1...v1.4.0) (2024-03-27)


### Features

* detect http / rpc errors as early as possible ([ad78669](https://github.com/hirosystems/chainhook/commit/ad78669204c7631af4f00ad0cadcb617bbff54d8))
* use stacks.rocksdb for predicate scan ([#514](https://github.com/hirosystems/chainhook/issues/514)) ([a4f1663](https://github.com/hirosystems/chainhook/commit/a4f16635dcd8cc6a7d4a3ce6608013007b78b0a5)), closes [#513](https://github.com/hirosystems/chainhook/issues/513) [#485](https://github.com/hirosystems/chainhook/issues/485)


### Bug Fixes

* enable debug logs in release mode ([#537](https://github.com/hirosystems/chainhook/issues/537)) ([fb49e28](https://github.com/hirosystems/chainhook/commit/fb49e28d3621a0db8d725a66985be3f18d99abee))
* improve error handling, and more! ([#524](https://github.com/hirosystems/chainhook/issues/524)) ([86b5c78](https://github.com/hirosystems/chainhook/commit/86b5c7859c8395a470e1b7d3249901624dc3c682)), closes [#498](https://github.com/hirosystems/chainhook/issues/498) [#521](https://github.com/hirosystems/chainhook/issues/521) [#404](https://github.com/hirosystems/chainhook/issues/404) [/github.com/hirosystems/chainhook/issues/517#issuecomment-1992135101](https://github.com/hirosystems//github.com/hirosystems/chainhook/issues/517/issues/issuecomment-1992135101) [#517](https://github.com/hirosystems/chainhook/issues/517) [#506](https://github.com/hirosystems/chainhook/issues/506) [#510](https://github.com/hirosystems/chainhook/issues/510) [#519](https://github.com/hirosystems/chainhook/issues/519)
* log errors on block download failure; implement max retries ([#503](https://github.com/hirosystems/chainhook/issues/503)) ([0fc38cb](https://github.com/hirosystems/chainhook/commit/0fc38cbce00a3a1cfde38e9d2b9d6eb984bdd8cd))
* **metrics:** update latest ingested block on reorg ([#515](https://github.com/hirosystems/chainhook/issues/515)) ([8f728f7](https://github.com/hirosystems/chainhook/commit/8f728f7e3f82306154478eceeb5c9d0ef4931028))
* order and filter blocks used to seed forking block pool ([#534](https://github.com/hirosystems/chainhook/issues/534)) ([a11bc1c](https://github.com/hirosystems/chainhook/commit/a11bc1c0f9120f11fa0a27cbeb336fd1fa78d7b3))
* seed forking handler with unconfirmed blocks to improve startup stability ([#505](https://github.com/hirosystems/chainhook/issues/505)) ([485394e](https://github.com/hirosystems/chainhook/commit/485394e9f3eb35089e0b0082ca0c23fbb0e9028f)), closes [#487](https://github.com/hirosystems/chainhook/issues/487)
* skip db consolidation if no new dataset was downloaded ([#513](https://github.com/hirosystems/chainhook/issues/513)) ([983a165](https://github.com/hirosystems/chainhook/commit/983a1658b52cb5b4a89ac46bb85f7355b346a1fb))
* update scan status for non-triggering predicates ([#511](https://github.com/hirosystems/chainhook/issues/511)) ([9073f42](https://github.com/hirosystems/chainhook/commit/9073f42285605ed7625039b3aae2316949dfc127)), closes [#498](https://github.com/hirosystems/chainhook/issues/498)

## [1.3.1](https://github.com/hirosystems/chainhook/compare/v1.3.0...v1.3.1) (2024-02-14)


### Bug Fixes

* add event index to transaction events ([#495](https://github.com/hirosystems/chainhook/issues/495)) ([d67d1e4](https://github.com/hirosystems/chainhook/commit/d67d1e405e34f5a6a97e057181d467ed1a208332)), closes [#417](https://github.com/hirosystems/chainhook/issues/417) [#387](https://github.com/hirosystems/chainhook/issues/387)
* correctly determine PoX vs PoB block commitments ([#499](https://github.com/hirosystems/chainhook/issues/499)) ([50dd26f](https://github.com/hirosystems/chainhook/commit/50dd26f19d1004a4ab60b4a67f1885cce89fc1e9)), closes [#496](https://github.com/hirosystems/chainhook/issues/496)

## [1.3.0](https://github.com/hirosystems/chainhook/compare/v1.2.1...v1.3.0) (2024-02-08)


### Features

* optionally serve Prometheus metrics ([#473](https://github.com/hirosystems/chainhook/issues/473)) ([67a38ac](https://github.com/hirosystems/chainhook/commit/67a38ac3c3777a52104b2eab4846a1adbc7d55dd))


### Bug Fixes

* adjust ordinal_number entry in ts client inscription transfer event, add new reveal data ([#476](https://github.com/hirosystems/chainhook/issues/476)) ([28bf5c4](https://github.com/hirosystems/chainhook/commit/28bf5c41723df5a186153f9cd626225adc261896))
* remove early return for event evaluation ([#484](https://github.com/hirosystems/chainhook/issues/484)) ([98f9e86](https://github.com/hirosystems/chainhook/commit/98f9e86187ba3e9534ca7d333936595a706179d0)), closes [#469](https://github.com/hirosystems/chainhook/issues/469)
* remove unreachable panic; return instead ([#490](https://github.com/hirosystems/chainhook/issues/490)) ([abe0fd5](https://github.com/hirosystems/chainhook/commit/abe0fd5b8b84352d081367477dadb3b8dc135a9b))
* use cli feature for `cargo chainhook-install` ([#486](https://github.com/hirosystems/chainhook/issues/486)) ([32f4d4e](https://github.com/hirosystems/chainhook/commit/32f4d4e6700be8aa8bf73740b8a2e590915b94df))
* validate predicate `start_block` and `end_block` ([#489](https://github.com/hirosystems/chainhook/issues/489)) ([e70025b](https://github.com/hirosystems/chainhook/commit/e70025bfd3d8f5588eb178781fdc87158245edb7)), closes [#477](https://github.com/hirosystems/chainhook/issues/477) [#464](https://github.com/hirosystems/chainhook/issues/464)

## [1.2.1](https://github.com/hirosystems/chainhook/compare/v1.2.0...v1.2.1) (2024-01-30)


### Bug Fixes

* reduce memory usage in archive file ingestion  ([#480](https://github.com/hirosystems/chainhook/issues/480)) ([83af58b](https://github.com/hirosystems/chainhook/commit/83af58bfdbbdcb5d310a8bcd0a6079325bac2804))

## [1.2.0](https://github.com/hirosystems/chainhook/compare/v1.1.1...v1.2.0) (2024-01-25)


### Features

* add bad request support ([7abe4f6](https://github.com/hirosystems/chainhook/commit/7abe4f6a70c39e91d6546e8f51cef8684344d4ff))
* add inscription transfer destination schema ([526de7a](https://github.com/hirosystems/chainhook/commit/526de7aba52bc3c82d8d627efab692e491174115))
* add jubilee support for inscription_revealed schemas ([#470](https://github.com/hirosystems/chainhook/issues/470)) ([823f430](https://github.com/hirosystems/chainhook/commit/823f4300c5b65ee006cdba1c6587fb549dcc1a33))
* add Wallet Descriptor Support for Transaction Indexing ([959da29](https://github.com/hirosystems/chainhook/commit/959da298b7cbf370e1b445bb82b50804c64d965f))
* broadcast ObserverEvent::BitcoinPredicateTriggered on successful requests ([6407e2c](https://github.com/hirosystems/chainhook/commit/6407e2cd6ea88f7fbc3452238404c63a59be8ac3))
* broadcast ObserverEvent::BitcoinPredicateTriggered on successful requests ([a6164ea](https://github.com/hirosystems/chainhook/commit/a6164ea05a77a1932418c02a002a7c3bf352caaf))
* introduce signet mode ([549c775](https://github.com/hirosystems/chainhook/commit/549c775bb5cdc0194c5a04d407e3a4cd5d92663b))


### Bug Fixes

* address review ([687e2ae](https://github.com/hirosystems/chainhook/commit/687e2ae7b367f3c1ec173e5c56b471945622540d))
* broken tests ([0e6359e](https://github.com/hirosystems/chainhook/commit/0e6359e66a90664c243c71cbc9f6114f318fbbcf))
* broken tests ([7a0209b](https://github.com/hirosystems/chainhook/commit/7a0209b480629e9c472e45e0803d01f9f208c779))
* buffer decoding of archive file to reduce memory usage ([#450](https://github.com/hirosystems/chainhook/issues/450)) ([f1b89f7](https://github.com/hirosystems/chainhook/commit/f1b89f7c9a05f1bc4cb59253ba63dadeca0e3b07)), closes [#401](https://github.com/hirosystems/chainhook/issues/401)
* build error ([88f597e](https://github.com/hirosystems/chainhook/commit/88f597e90c662427b18a6d20cfbcf3d931b3bb35))
* enable default features for hiro-system-kit ([867424a](https://github.com/hirosystems/chainhook/commit/867424a5c060cdd314d6d35cd27bcea9bd3690be))
* skip empty chunks when decoding gz ([b4ce82f](https://github.com/hirosystems/chainhook/commit/b4ce82f92da49a67a55483a7d4cba283781713e0))
* **stacks-indexer:** prevent subtract with overflow ([#449](https://github.com/hirosystems/chainhook/issues/449)) ([d8d9979](https://github.com/hirosystems/chainhook/commit/d8d9979823070dcef37a3556c99bc34b1d48e27c))
* update ordhook URLs on typescript client ([9462ae3](https://github.com/hirosystems/chainhook/commit/9462ae3b20ff6e49c4e649c370a9ad97102f0cb4))
* warnings ([126d049](https://github.com/hirosystems/chainhook/commit/126d0499c13a2ff6d4e36d00c90281f3ef5d1138))

## [1.1.1](https://github.com/hirosystems/chainhook/compare/v1.1.0...v1.1.1) (2023-10-11)


### Bug Fixes

* add auth header for stacks hook ([#444](https://github.com/hirosystems/chainhook/issues/444)) ([8c4e5ea](https://github.com/hirosystems/chainhook/commit/8c4e5ea8b54b6b20d3b19796c9d0b57f3d38a3a2)), closes [#438](https://github.com/hirosystems/chainhook/issues/438)
* don't evaluate transactions for block predicates ([#445](https://github.com/hirosystems/chainhook/issues/445)) ([0e84fe7](https://github.com/hirosystems/chainhook/commit/0e84fe7e2b6098345eee4b997138e6910a849996))
* redis conn ([#442](https://github.com/hirosystems/chainhook/issues/442)) ([80737ad](https://github.com/hirosystems/chainhook/commit/80737addce9d6df7035b5586da11f33640ee72d2))

## [1.1.0](https://github.com/hirosystems/chainhook/compare/v1.0.0...v1.1.0) (2023-10-10)


### Features

* allow matching with regex for stacks print_event ([#380](https://github.com/hirosystems/chainhook/issues/380)) ([131809e](https://github.com/hirosystems/chainhook/commit/131809e7d2b8e4b48b83114440a4876ec9aee9ee)), closes [#348](https://github.com/hirosystems/chainhook/issues/348)
* augment predicate status returned by GET/LIST endpoints ([#397](https://github.com/hirosystems/chainhook/issues/397)) ([a100263](https://github.com/hirosystems/chainhook/commit/a100263a0bcab3a43c9bbce49ddead754d2d621c)), closes [#396](https://github.com/hirosystems/chainhook/issues/396) [#324](https://github.com/hirosystems/chainhook/issues/324) [#390](https://github.com/hirosystems/chainhook/issues/390) [#402](https://github.com/hirosystems/chainhook/issues/402) [#403](https://github.com/hirosystems/chainhook/issues/403)
* introduce "data_handler_tx" ([ee486f3](https://github.com/hirosystems/chainhook/commit/ee486f3571f97728d5305bdb72a303134fca1bf5))


### Bug Fixes

* build error ([85d4d91](https://github.com/hirosystems/chainhook/commit/85d4d91ca6276a25d0bc95e256da356758155466))
* build errors ([b9ff1aa](https://github.com/hirosystems/chainhook/commit/b9ff1aab26a26b9ada1e19d12a891fa2e8ad72fd))
* build errro ([be0c229](https://github.com/hirosystems/chainhook/commit/be0c22957b7345721e33d38e3bfa98794155e7a7))
* bump retries and delays ([aff3690](https://github.com/hirosystems/chainhook/commit/aff36904e557026ab91a039e40959957b5bbc309))
* chainhook not being registered ([5a809c6](https://github.com/hirosystems/chainhook/commit/5a809c63bec1c949314ecbd44ef1348286968dec))
* ensure that the parent block was previously received. else, fetch it ([2755266](https://github.com/hirosystems/chainhook/commit/275526620209e8b7137722f9c081aa7b9dca31e5))
* migrate to finer zmq lib ([4eb5a07](https://github.com/hirosystems/chainhook/commit/4eb5a07ad350360f159b5443d0b2d665c20892bf))
* prevent panic when scanning from genesis block ([#408](https://github.com/hirosystems/chainhook/issues/408)) ([1868a06](https://github.com/hirosystems/chainhook/commit/1868a06aba6de61bfb516b0f88b3e900a5d99a64))
* remove event_handlers ([6fecfd2](https://github.com/hirosystems/chainhook/commit/6fecfd2f41fe5bc8c672a51bcf3050c634927b84))
* retrieve blocks until tip ([5213f5f](https://github.com/hirosystems/chainhook/commit/5213f5f67a8adfddc72de7c707eb9d0de46150a2))
* revisit approach ([67a34dc](https://github.com/hirosystems/chainhook/commit/67a34dcb2f7dab546bb88bd1a6ed098109953531))
* use crossbeam channels ([ea33553](https://github.com/hirosystems/chainhook/commit/ea335530c174b8893013e6be7e0258285c4a9667))
* workflow ([d434c93](https://github.com/hirosystems/chainhook/commit/d434c9362ec46b13f1a98d51f62d1c1938f70319))

#### 1.4.0 (2023-01-23)

##### New Features

*  Polish LSP completion capability ([4cc24ed3](https://github.com/hirosystems/clarinet/commit/4cc24ed3c5edaf61d057c4c1e1ab3d32957e6a15), [16db8dd4](https://github.com/hirosystems/clarinet/commit/16db8dd454ddc5acaec1161ef4aba26cba4c37bf), [905e5433](https://github.com/hirosystems/clarinet/commit/905e5433cc7bf208ea480cc148865e8198bb0420), [9ffdad0f](https://github.com/hirosystems/clarinet/commit/9ffdad0f46294dd36c83ab92c3241b2b01499576), [d3a27933](https://github.com/hirosystems/clarinet/commit/d3a2793350e96ad224f038b11a6ada602fef46af), [cad54358](https://github.com/hirosystems/clarinet/commit/cad54358a1978ab4953aca9e0f3a6ff52ac3afc4), [439c4933](https://github.com/hirosystems/clarinet/commit/439c4933bcbeaaec9f3413892bbcc12fc8ec1b15))
*  Upgrade clarity vm ([fefdd1e0](https://github.com/hirosystems/clarinet/commit/fefdd1e092dad8e546e2db7683202d81dd91407a))
*  Upgrade stacks-node next image ([492804bb](https://github.com/hirosystems/clarinet/commit/492804bb472a950dded1b1d0c8a951b434a141ac))
*  Expose stacks-node settings wait_time_for_microblocks, first_attempt_time_ms, subsequent_attempt_time_ms in Devnet config file
*  Improve Epoch 2.1 deployments handling
*  Improve `stacks-devnet-js` stability

##### Documentation

*  Updated documentation to set clarity version of contract ([b124d96f](https://github.com/hirosystems/clarinet/commit/b124d96fbbef29befc26601cdbd8ed521d4a162a))


# [1.3.1](https://github.com/hirosystems/clarinet/compare/v1.3.0...v1.3.1) (2023-01-03)

### New Features

*  Introduce use_docker_gateway_routing setting for CI environments
*  Improve signature help in LSP ([eee03cff](https://github.com/hirosystems/clarinet/commit/eee03cff757d3e288abe7436eca06d4c440c71dc))
*  Add support for more keyword help in REPL ([f564d469](https://github.com/hirosystems/clarinet/commit/f564d469ccf5e79ab924643627fdda8715da6a1d, [0efcc75e](https://github.com/hirosystems/clarinet/commit/0efcc75e7da3b801e1a862094791f3747452f9e0))
*  Various Docker management optimizations / fixes ([b379d29f](https://github.com/hirosystems/clarinet/commit/b379d29f4ad4e85df42e804bc00cec2baff375c0), [4f4c8806](https://github.com/hirosystems/clarinet/commit/4f4c88064e2045de9e48d75b507dd321d4543046))

### Bug Fixes

*  Fix STX assets title ([fdc748e7](https://github.com/hirosystems/clarinet/commit/fdc748e7b7df6ef1a6b62ab5cb8c1b68bde9b1ad), [ce5d107c](https://github.com/hirosystems/clarinet/commit/ce5d107c76950d989eb0be8283adf35930283f18))
*  Fix define function grammar ([d02835ba](https://github.com/hirosystems/clarinet/commit/d02835bab06578eebb13a791f9faa1c2571d3fb9))
*  Fix get_costs panicking ([822d8e29](https://github.com/hirosystems/clarinet/commit/822d8e29965e11864f708a1efd7a8ad385bc1ba3), [e41ae715](https://github.com/hirosystems/clarinet/commit/e41ae71585a432d21cc16c109d2858f9e1d8e22b))
