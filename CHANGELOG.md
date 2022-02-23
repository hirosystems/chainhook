# [0.23.0](https://github.com/hirosystems/clarity-repl/compare/v0.22.2...v0.23.0) (2022-02-23)


### Bug Fixes

* report an error for CRLF line-endings ([5a4ccf0](https://github.com/hirosystems/clarity-repl/commit/5a4ccf083e3965569749d39b4ccd9345b93cdf22)), closes [#98](https://github.com/hirosystems/clarity-repl/issues/98)


### Features

* add note about CRLF -> LF mode ([5c1d2b6](https://github.com/hirosystems/clarity-repl/commit/5c1d2b6498b7fb0f6527cfd2c67b8d76e9775507))

## [0.22.2](https://github.com/hirosystems/clarity-repl/compare/v0.22.1...v0.22.2) (2022-02-18)


### Bug Fixes

* rustls was not properly enabled (openssl c lib was being used) ([4f6b7b5](https://github.com/hirosystems/clarity-repl/commit/4f6b7b5284abb0a37b0338d78e0853bfc1459d17))

## [0.22.1](https://github.com/hirosystems/clarity-repl/compare/v0.22.0...v0.22.1) (2022-02-12)


### Bug Fixes

* append output from initial contracts ([7dc1a8e](https://github.com/hirosystems/clarity-repl/commit/7dc1a8ee076227ca23e78b3e83db8d71f1033f36))

# [0.22.0](https://github.com/hirosystems/clarity-repl/compare/v0.21.0...v0.22.0) (2022-02-09)


### Bug Fixes

* add checks for argument counts to map-* funcs ([1a1cadb](https://github.com/hirosystems/clarity-repl/commit/1a1cadb876f281b732801455334167a17cd84ac7)), closes [stacks-network/stacks-blockchain#3018](https://github.com/stacks-network/stacks-blockchain/issues/3018) [hirosystems/clarinet#228](https://github.com/hirosystems/clarinet/issues/228)
* allow symbols in identifiers ([15acc61](https://github.com/hirosystems/clarity-repl/commit/15acc61d4bd9e31235608de08514f2900eab7578))
* crash when an error is reported at EOF ([af6894a](https://github.com/hirosystems/clarity-repl/commit/af6894a2934973298df2bd16500bcbb4c53d4512))
* disabling requirements on wasm builds ([9176e2b](https://github.com/hirosystems/clarity-repl/commit/9176e2b61b79e1b21e70dcb7fce2699938866495))
* fix bug in comment handling ([6dd45de](https://github.com/hirosystems/clarity-repl/commit/6dd45dea7224e8e690b5f49da8835f207294de1a))
* fix crash on error with 0 column ([0ee66b9](https://github.com/hirosystems/clarity-repl/commit/0ee66b900410800dddd4edb861f15e0a673f798e))
* fix error when handling an invalid symbol ([70cfa1a](https://github.com/hirosystems/clarity-repl/commit/70cfa1ae63016500761ca540cf88b31fd9e044dd))
* fix handling of filtered params ([4d6d222](https://github.com/hirosystems/clarity-repl/commit/4d6d2227a2e15ae22f0858c19d7be770e603f846))
* fix handling of negative integer literals ([edb4d14](https://github.com/hirosystems/clarity-repl/commit/edb4d145f388131e6c62cabb48c6ac7148611c89))
* fix lexer error with empty comment ([ae896b5](https://github.com/hirosystems/clarity-repl/commit/ae896b5006f2fabdb8fba4895bf8a5c0da611cab))
* improve handling of invalid trait reference ([5aa363a](https://github.com/hirosystems/clarity-repl/commit/5aa363a8b2f5beaf872c9401fc348d9c5482b60b))
* improved handling of unterminated strings ([5035a2f](https://github.com/hirosystems/clarity-repl/commit/5035a2ff5db95b2abcd5d8f27a69ed24e63629b2))
* return more errors ([a44e35d](https://github.com/hirosystems/clarity-repl/commit/a44e35d67d1274899601e4b62cb01bc9486586c6))
* returns all the diagnostics ([dc992a3](https://github.com/hirosystems/clarity-repl/commit/dc992a3eba4c59586c8ba538365532bfdf21f51d))


### Features

* ability to lazy load contracts ([bc50b26](https://github.com/hirosystems/clarity-repl/commit/bc50b268bd61cb32710d4dd4418f21e1ac624d1c))
* add ability to save contracts ([f43abb5](https://github.com/hirosystems/clarity-repl/commit/f43abb585e10db298f882c8f9667dafd365513ae))
* add disk cache for contracts ([a036fda](https://github.com/hirosystems/clarity-repl/commit/a036fda0780fb0ca96635910f424d8ec28a7cc7a))
* add option to select parser version ([c731e56](https://github.com/hirosystems/clarity-repl/commit/c731e5675e06690d978c3f9a6629f25dba05f6a9))
* checker support of trusted sender/caller ([70191a4](https://github.com/hirosystems/clarity-repl/commit/70191a4fbda4aaf45f53f26a9c5ea6558c0ed565)), closes [#62](https://github.com/hirosystems/clarity-repl/issues/62)
* cleanup configuration of repl and analysis ([ce389c1](https://github.com/hirosystems/clarity-repl/commit/ce389c1ba94935dec34b54cf650188b2a06c3569))
* improve check-checker handling of rollbacks ([cc0c3e2](https://github.com/hirosystems/clarity-repl/commit/cc0c3e2bbc59c85ad4cf9b141d9e071a12af08c9)), closes [#81](https://github.com/hirosystems/clarity-repl/issues/81)
* improved parser ([e7ae7b8](https://github.com/hirosystems/clarity-repl/commit/e7ae7b813542a9be512c87fbd37f9b16d8009198)), closes [#74](https://github.com/hirosystems/clarity-repl/issues/74)

# [0.21.0](https://github.com/hirosystems/clarity-repl/compare/v0.20.1...v0.21.0) (2022-01-13)


### Bug Fixes

* fix ast visitor traversal of contract-of expr ([d553e50](https://github.com/hirosystems/clarity-repl/commit/d553e50d3ffdac6b4994015450058a3a29e872ed)), closes [#77](https://github.com/hirosystems/clarity-repl/issues/77)
* resolve CI failure for forks ([8152e4b](https://github.com/hirosystems/clarity-repl/commit/8152e4b086faef02ac21f23b8af5d65c93345166))


### Features

* add 'filter' annotation ([4cebe6b](https://github.com/hirosystems/clarity-repl/commit/4cebe6bcc58c928ef62a3d3faad6d15802f215db)), closes [#72](https://github.com/hirosystems/clarity-repl/issues/72)

## [0.20.1](https://github.com/hirosystems/clarity-repl/compare/v0.20.0...v0.20.1) (2022-01-06)


### Bug Fixes

* remove println events ([4879ee4](https://github.com/hirosystems/clarity-repl/commit/4879ee426655b43f04b12492b41543d5ad486fb9))

# [0.20.0](https://github.com/hirosystems/clarity-repl/compare/v0.19.0...v0.20.0) (2022-01-05)


### Bug Fixes

* properly update block id lookup table when advancing the chain tip ([d457df5](https://github.com/hirosystems/clarity-repl/commit/d457df5270b04356bbc382c0d2fb2baa929c5308))
* snippet use in LSP ([f4dccdf](https://github.com/hirosystems/clarity-repl/commit/f4dccdfc1820108ec23f321ac404151720af21df))


### Features

* **check-checker:** allow private function filter ([6036d69](https://github.com/hirosystems/clarity-repl/commit/6036d6997dc9ffd38d98a5fddf85626213b1682d))

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
