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
