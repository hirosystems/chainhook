# [0.19.0](https://github.com/hirosystems/clarity-repl/compare/v0.18.0...v0.19.0) (2021-12-21)


### Bug Fixes

* chain tip logic and vrf seed generation ([1863e00](https://github.com/hirosystems/clarity-repl/commit/1863e00ec0c0391610f2cf1635f048a82f40052e))
* correctly utilize current_chain_tip ([b134d39](https://github.com/hirosystems/clarity-repl/commit/b134d39fc56e7ddd1a8152d25ec2a6f700f13de2))
* panic if block doesn't exist ([2aedd35](https://github.com/hirosystems/clarity-repl/commit/2aedd352069488452349d6b2246936c14c2661ea))
* use lookup table to make datastore more efficient ([ad1cfae](https://github.com/hirosystems/clarity-repl/commit/ad1cfaee29aa7d811c83f9db6b9c3defe3eb0cb1))


### Features

* start making Datastore block aware ([ca1e097](https://github.com/hirosystems/clarity-repl/commit/ca1e09733fddff3a07d9619ee4d165a2c29a7fa6))
* use hash for block id ([2ab9ed6](https://github.com/hirosystems/clarity-repl/commit/2ab9ed603d320bd86db9fbec15b187e48d5be1b7))

# [0.18.0](https://github.com/hirosystems/clarity-repl/compare/v0.17.0...v0.18.0) (2021-12-17)


### Bug Fixes

* fix bug in handling of map-insert/set ([7b47da1](https://github.com/hirosystems/clarity-repl/commit/7b47da1efcaf80f17f5dcb2a0dbf9557fa078d5c))
* fix unit tests after 351ad77 ([af6a3f4](https://github.com/hirosystems/clarity-repl/commit/af6a3f464d2dbf920b8d15062405f3143f51998c))
* handle private functions in check-checker ([b73ad7b](https://github.com/hirosystems/clarity-repl/commit/b73ad7b03fff169436fb7c794bf6bed713d067f6))
* order taint info diagnostics ([e4c4211](https://github.com/hirosystems/clarity-repl/commit/e4c42113d9ffe22b9c3a3b4bc1ad77c1413bdca4))
* proposal for extra logs ([e72bc97](https://github.com/hirosystems/clarity-repl/commit/e72bc976356eacd48121ac66f0f435c4a1753631))
* set costs_version ([54bd48c](https://github.com/hirosystems/clarity-repl/commit/54bd48c77520b2408ca53bdc003a37ec25807856))
* **taint:** fix bug in taint propagation ([4a5579e](https://github.com/hirosystems/clarity-repl/commit/4a5579efe1072ba4282b04b38dc320893ec3d2c1))
* use contract name in diagnostic output ([45b9993](https://github.com/hirosystems/clarity-repl/commit/45b9993efbcf2484ec5f63cac9e84656f030a4c9))


### Features

* add `analysis` field to settings ([ef0d186](https://github.com/hirosystems/clarity-repl/commit/ef0d186cb4ec716e8a576ff964cf7711b185bba1))
* add support for annotations ([4b10465](https://github.com/hirosystems/clarity-repl/commit/4b104651a9d9768e03bb767865a1ff2f2dee3489))
* **analysis:** add taint checker pass ([f03f20a](https://github.com/hirosystems/clarity-repl/commit/f03f20a7d74e928e3b6c1a3df40991b98f4ca503)), closes [#33](https://github.com/hirosystems/clarity-repl/issues/33)
* **analysis:** improve diagnostics ([2eea11a](https://github.com/hirosystems/clarity-repl/commit/2eea11a7a3855aba23977923acc51ee1ad57c0e1))
* check argument count to user-defined funcs ([ceff88a](https://github.com/hirosystems/clarity-repl/commit/ceff88ac58f379e78b10e33947504de14b6d8805)), closes [#47](https://github.com/hirosystems/clarity-repl/issues/47)
* check for unchecked trait in contract-call? ([fec4149](https://github.com/hirosystems/clarity-repl/commit/fec4149e4317f7a9ea4da0fb4da925c7659f5793))
* invoke binary with clarity code ([264931e](https://github.com/hirosystems/clarity-repl/commit/264931e143ab45fcbf81faa7c6890dfe36c39088))
* remove warnings for txns on sender's assets ([2922e5c](https://github.com/hirosystems/clarity-repl/commit/2922e5c6dda668b1710a660666d02563a2bb0851))
* report warning for tainted return value ([137c806](https://github.com/hirosystems/clarity-repl/commit/137c806b3107e278d19d0425af6b45f4f62a4e56))
* update costs with final values ([b36196a](https://github.com/hirosystems/clarity-repl/commit/b36196aa55fd34c2705ee21364b79949590ba969))
* update default costs ([00e3328](https://github.com/hirosystems/clarity-repl/commit/00e332820441b851e8c60da34184e83bbe25daf5))

# [0.17.0](https://github.com/hirosystems/clarity-repl/compare/v0.16.0...v0.17.0) (2021-11-17)


### Bug Fixes

* ignore RUSTSEC-2021-0124 ([65a494a](https://github.com/hirosystems/clarity-repl/commit/65a494ad2e761a729653b127882034cec9f465ff))


### Features

* add encode/decode commands ([cfea2e8](https://github.com/hirosystems/clarity-repl/commit/cfea2e8fa3e330dfd610a2516d2cc1918ccf6361)), closes [#7](https://github.com/hirosystems/clarity-repl/issues/7)
