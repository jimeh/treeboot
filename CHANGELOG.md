# Changelog

## [0.10.0](https://github.com/jimeh/treeboot/compare/v0.9.0...v0.10.0) (2026-07-13)


### ⚠ BREAKING CHANGES

* public operational enums in treeboot-core are now non-exhaustive. Downstream crates that exhaustively match these enums will no longer compile and must add a wildcard arm or otherwise handle unknown future variants. The CLI and configuration contract are unchanged.

### Features

* preserve public enum extensibility ([#92](https://github.com/jimeh/treeboot/issues/92)) ([0a72649](https://github.com/jimeh/treeboot/commit/0a726494ca40c77cf35d8cd08a4133434254ded7))


### Bug Fixes

* allow Renovate to complete Rust updates ([#101](https://github.com/jimeh/treeboot/issues/101)) ([6d62f86](https://github.com/jimeh/treeboot/commit/6d62f865747c43f60a551ccca8e4ef04c3a764b5))
* keep musl release smoke tests runnable ([#113](https://github.com/jimeh/treeboot/issues/113)) ([0ca82fe](https://github.com/jimeh/treeboot/commit/0ca82fe881f554b21d1c23ade89e649e42c5be20))
* keep Renovate from rewriting mise constraints ([#107](https://github.com/jimeh/treeboot/issues/107)) ([5ba2812](https://github.com/jimeh/treeboot/commit/5ba2812276374ffc92b408110a904ddbb995462d))
* keep Rust toolchain lockfile updates in sync ([#98](https://github.com/jimeh/treeboot/issues/98)) ([ba6acf2](https://github.com/jimeh/treeboot/commit/ba6acf2a1a04233d0ba728fec8e8efb45d011f3a))
* keep treeboot mise versions normalized ([#112](https://github.com/jimeh/treeboot/issues/112)) ([45b00ed](https://github.com/jimeh/treeboot/commit/45b00edec552d9d09afcdb6a53d6fef03c58c049))
* let Renovate refresh the Rust lockfile ([#104](https://github.com/jimeh/treeboot/issues/104)) ([d3836a5](https://github.com/jimeh/treeboot/commit/d3836a580150d0774355217454321d5a30ee35d1))
* make Renovate update mise tools reliably ([#111](https://github.com/jimeh/treeboot/issues/111)) ([3b25c08](https://github.com/jimeh/treeboot/commit/3b25c088ed679902397c92f005265bf0f8a4c752))
* preserve native Git worktree paths ([#90](https://github.com/jimeh/treeboot/issues/90)) ([c05c814](https://github.com/jimeh/treeboot/commit/c05c8147a3d1b6be8f096ef649ffa6733599e037))
* prevent command cwd escapes after file operations ([#91](https://github.com/jimeh/treeboot/issues/91)) ([cae33b6](https://github.com/jimeh/treeboot/commit/cae33b60c7ecf70434f6e2ed7f2ba66ad72fbce9))
* update fuzzy mise locks without changing constraints ([#110](https://github.com/jimeh/treeboot/issues/110)) ([f74fea3](https://github.com/jimeh/treeboot/commit/f74fea346748643a453b5451fd40323f5d254e93))
* update mise locks without rewriting constraints ([#108](https://github.com/jimeh/treeboot/issues/108)) ([efee101](https://github.com/jimeh/treeboot/commit/efee1016e048b9801c8f4bb4ca149bb885aa5430))

## [0.9.0](https://github.com/jimeh/treeboot/compare/v0.8.2...v0.9.0) (2026-07-04)


### Features

* add include rules to copy and sync operations ([#88](https://github.com/jimeh/treeboot/issues/88)) ([462a53c](https://github.com/jimeh/treeboot/commit/462a53ca9af4c9e811334824ae3785073e7ca2e5))

## [0.8.2](https://github.com/jimeh/treeboot/compare/v0.8.1...v0.8.2) (2026-06-30)


### Bug Fixes

* normalize Windows paths consistently ([#81](https://github.com/jimeh/treeboot/issues/81)) ([1ede1ee](https://github.com/jimeh/treeboot/commit/1ede1ee1630bf08d09b1c46ad0deacf78c660332))

## [0.8.1](https://github.com/jimeh/treeboot/compare/v0.8.0...v0.8.1) (2026-06-29)


### Bug Fixes

* handle root checkout symlink targets across path aliases ([#77](https://github.com/jimeh/treeboot/issues/77)) ([cc5e11d](https://github.com/jimeh/treeboot/commit/cc5e11d70de82d45ac92d3fbd8ead64afff7a86a))

## [0.8.0](https://github.com/jimeh/treeboot/compare/v0.7.0...v0.8.0) (2026-06-29)


### ⚠ BREAKING CHANGES

* `treeboot-core` replaces the old `RuntimePolicy` type alias with a runtime precedence model, moves `RuntimeOptionOverrides` into the runtime API, and replaces `Reporter` file-operation lifecycle callbacks with structured `OutputEvent::FileOperation*` events.

### Features

* add strict diagnostics to treeboot doctor ([#75](https://github.com/jimeh/treeboot/issues/75)) ([aa1fa50](https://github.com/jimeh/treeboot/commit/aa1fa50c30629fdde3fbe39d3e2055abdc713146))

## [0.7.0](https://github.com/jimeh/treeboot/compare/v0.6.0...v0.7.0) (2026-06-27)


### Features

* add ignore rules for copy and sync ([#64](https://github.com/jimeh/treeboot/issues/64)) ([182c0f6](https://github.com/jimeh/treeboot/commit/182c0f62295c4ee9dad14b049098400061968560))

## [0.6.0](https://github.com/jimeh/treeboot/compare/v0.5.1...v0.6.0) (2026-06-26)


### ⚠ BREAKING CHANGES

* **treeboot-core:** treeboot-core command-shaped option defaults no longer use ambient process environment. Public option structs now require explicit EnvironmentInput, and RuntimeOptionOverrides::from_env() has been replaced by RuntimeOptionOverrides::from_environment(&EnvironmentInput) or RuntimeOptionOverrides::from_process_env().

### Bug Fixes

* avoid false checksum-sync change on short reads ([#57](https://github.com/jimeh/treeboot/issues/57)) ([a20ac67](https://github.com/jimeh/treeboot/commit/a20ac671639ed896c5bc0a23a226f21bf05406ad))
* enforce validated action plan boundaries ([#60](https://github.com/jimeh/treeboot/issues/60)) ([e8f641b](https://github.com/jimeh/treeboot/commit/e8f641b1a8b81621b7ec56c6f8de19e45be71211))
* recheck preserved source symlinks before apply ([#62](https://github.com/jimeh/treeboot/issues/62)) ([57c0888](https://github.com/jimeh/treeboot/commit/57c0888d45f8b57a92897050a1a297309f3aa7d3))


### Code Refactoring

* **treeboot-core:** make core environment input explicit ([#61](https://github.com/jimeh/treeboot/issues/61)) ([8e61a7f](https://github.com/jimeh/treeboot/commit/8e61a7f5b8d8e584c74fcf4a77f5ad0b986dd9b3))

## [0.5.1](https://github.com/jimeh/treeboot/compare/v0.5.0...v0.5.1) (2026-06-26)


### Bug Fixes

* cover Windows ARM64 runner setup ([#55](https://github.com/jimeh/treeboot/issues/55)) ([04df08b](https://github.com/jimeh/treeboot/commit/04df08bc4e7530bae51a7308aacd3c56afca9b94))

## [0.5.0](https://github.com/jimeh/treeboot/compare/v0.4.1...v0.5.0) (2026-06-26)


### Features

* add treeboot inspection commands ([#50](https://github.com/jimeh/treeboot/issues/50)) ([d04ae44](https://github.com/jimeh/treeboot/commit/d04ae448d485a362a2d2f6e48534e994d8db0cb5))
* make file operation output compact by default ([#53](https://github.com/jimeh/treeboot/issues/53)) ([872e80d](https://github.com/jimeh/treeboot/commit/872e80da76d479c057fac3da5ae0ea9bd92a3876))
* preserve copy and sync metadata by default ([#54](https://github.com/jimeh/treeboot/issues/54)) ([671baf3](https://github.com/jimeh/treeboot/commit/671baf3cfb399268dd077b749534ffc25daf575f))

## [0.4.1](https://github.com/jimeh/treeboot/compare/v0.4.0...v0.4.1) (2026-06-23)


### Bug Fixes

* keep Linux installers from selecting Android assets ([#47](https://github.com/jimeh/treeboot/issues/47)) ([9c8447a](https://github.com/jimeh/treeboot/commit/9c8447a97f967e97d24879d5ea7faed3f3422447))

## [0.4.0](https://github.com/jimeh/treeboot/compare/v0.3.0...v0.4.0) (2026-06-23)


### Features

* add status command for worktree discovery details ([#42](https://github.com/jimeh/treeboot/issues/42)) ([f358686](https://github.com/jimeh/treeboot/commit/f3586867990f0095f50f22538847645464c99a57))

## [0.3.0](https://github.com/jimeh/treeboot/compare/v0.2.0...v0.3.0) (2026-06-22)


### Features

* harden treeboot setup boundaries ([#39](https://github.com/jimeh/treeboot/issues/39)) ([8da560c](https://github.com/jimeh/treeboot/commit/8da560c6636457d17214ec2d3d05d330596da2ff))


### Bug Fixes

* reject overlapping file operation targets ([#35](https://github.com/jimeh/treeboot/issues/35)) ([e221130](https://github.com/jimeh/treeboot/commit/e221130a85e7959974bb920dde994d7d78f7ff96))

## [0.2.0](https://github.com/jimeh/treeboot/compare/v0.1.0...v0.2.0) (2026-06-21)


### Features

* add manual file operations ([#12](https://github.com/jimeh/treeboot/issues/12)) ([c95dd90](https://github.com/jimeh/treeboot/commit/c95dd900666e64185a8e529ec36fc056ceca6980))
* align config runtime options with spec ([#7](https://github.com/jimeh/treeboot/issues/7)) ([c5d273f](https://github.com/jimeh/treeboot/commit/c5d273f16596eab40b76a9d3beb71813bda04f56))
* align implementation with spec v1.2 ([#8](https://github.com/jimeh/treeboot/issues/8)) ([088d145](https://github.com/jimeh/treeboot/commit/088d145025d20c029a32c6844ec4b5f593ad694c))
* default init to config output ([#23](https://github.com/jimeh/treeboot/issues/23)) ([8d16ad0](https://github.com/jimeh/treeboot/commit/8d16ad00f9d9e447862c2d798ddccf3df9b09bad))
* establish milestone 1 run flow ([4f5d596](https://github.com/jimeh/treeboot/commit/4f5d59611dfeb99ce7a6eca7806e8cc29fdce97b))
* establish milestone 1 run flow ([3d06835](https://github.com/jimeh/treeboot/commit/3d06835d1f5bfa636ed06a2d6e8d284745de9047))
* execute declarative commands after file operations ([#10](https://github.com/jimeh/treeboot/issues/10)) ([df51346](https://github.com/jimeh/treeboot/commit/df5134693de9b624a8bb17cfa5aa91d8f55953ac))
* generate shell completions from the CLI ([#11](https://github.com/jimeh/treeboot/issues/11)) ([d463006](https://github.com/jimeh/treeboot/commit/d463006dce322e1b492766b1065e32206f079a1b))
* implement declarative file operations ([#9](https://github.com/jimeh/treeboot/issues/9)) ([a41c183](https://github.com/jimeh/treeboot/commit/a41c1836043afa6909947989f077d914c9066bb3))
* implement declarative validation planning ([#6](https://github.com/jimeh/treeboot/issues/6)) ([791c08c](https://github.com/jimeh/treeboot/commit/791c08c536b9b117c0e26a816c128c7d5a11ed56))
* implement milestone 2 config parsing ([6942d26](https://github.com/jimeh/treeboot/commit/6942d2667b545b8dffd5f3d2125b889d0ddf9ff4))
* implement milestone 2 config parsing ([be4e155](https://github.com/jimeh/treeboot/commit/be4e155e4a98e869f680caf5e883250d39007ea2))
* surface normalized config text fields ([#19](https://github.com/jimeh/treeboot/issues/19)) ([0a19d5c](https://github.com/jimeh/treeboot/commit/0a19d5c7fd42537cf8a6b2fc08e129a459b3d245))


### Bug Fixes

* skip bootstrap from root checkout ([#4](https://github.com/jimeh/treeboot/issues/4)) ([09d8974](https://github.com/jimeh/treeboot/commit/09d8974a71f85ad2517a120b086031f2cb0fcc18))
