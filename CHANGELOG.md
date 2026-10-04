# Changelog

## [0.7.1](https://github.com/sunerpy/lockra/compare/v0.7.0...v0.7.1) (2026-10-04)


### Features

* **mobile:** 有指纹的手机默认开启指纹解锁 ([#48](https://github.com/sunerpy/lockra/issues/48)) ([9947ca7](https://github.com/sunerpy/lockra/commit/9947ca758c3e730aa66a5ead8f59ae83a05213b5))


### Bug Fixes

* **desktop:** macOS 应用内更新后不再弹出钥匙串询问 ([#50](https://github.com/sunerpy/lockra/issues/50)) ([732d70f](https://github.com/sunerpy/lockra/commit/732d70fea62320e0ec086d7ef1b22fe04b8c16c6))


### Documentation

* **release:** 记录只升补丁号的发布方法 ([#51](https://github.com/sunerpy/lockra/issues/51)) ([368170c](https://github.com/sunerpy/lockra/commit/368170c6633f9ffec423f261eb290a19cd37ced5))

## [0.7.0](https://github.com/sunerpy/lockra/compare/v0.6.0...v0.7.0) (2026-10-03)


### Features

* **mobile:** 增加 Android 应用骨架 ([#31](https://github.com/sunerpy/lockra/issues/31)) ([9c4786d](https://github.com/sunerpy/lockra/commit/9c4786d32ea35bedb962daccc9f707465aa3dd45))
* **mobile:** 手机端保存备份与从备份恢复 ([#36](https://github.com/sunerpy/lockra/issues/36)) ([8405df3](https://github.com/sunerpy/lockra/commit/8405df308130aa2892dd4b334298f415a17dc588))
* **mobile:** 手机端同步 ([#39](https://github.com/sunerpy/lockra/issues/39)) ([b1aed14](https://github.com/sunerpy/lockra/commit/b1aed14bb5f0ef82dc57a44077452f0f5d5e2e65))
* **mobile:** 手机端导出账号到其他验证器 ([#37](https://github.com/sunerpy/lockra/issues/37)) ([709a5d3](https://github.com/sunerpy/lockra/commit/709a5d38efa16d23c67b60cd1fb2897b807f8f03))
* **mobile:** 手机端手动检查更新 ([#42](https://github.com/sunerpy/lockra/issues/42)) ([bea4200](https://github.com/sunerpy/lockra/commit/bea4200cf440afa47b46e81eb7e7762e8bfae789))
* **mobile:** 手机端添加、编辑与删除账号 ([#33](https://github.com/sunerpy/lockra/issues/33)) ([1f3cc2a](https://github.com/sunerpy/lockra/commit/1f3cc2ab1998cd42123897b0e1094e76886ff5a2))
* **mobile:** 手机端用指纹解锁 ([#38](https://github.com/sunerpy/lockra/issues/38)) ([389776e](https://github.com/sunerpy/lockra/commit/389776e25c274150b2f05101e1426f323fb6a725))
* **mobile:** 手机端设置页与从文件导入 ([#35](https://github.com/sunerpy/lockra/issues/35)) ([3409610](https://github.com/sunerpy/lockra/commit/3409610c3fce02b0df91e70d8d55e84e4dfc553a))
* **mobile:** 用相机扫码或从照片读取二维码导入账号 ([#34](https://github.com/sunerpy/lockra/issues/34)) ([809d155](https://github.com/sunerpy/lockra/commit/809d155875aa119ac298fcd0fec4f234a242be28))
* **unlock:** 可把 Touch ID、Windows Hello 或指纹设为默认解锁方式 ([#41](https://github.com/sunerpy/lockra/issues/41)) ([78a7151](https://github.com/sunerpy/lockra/commit/78a715182287374f7fd0a1f3fce9746dfe076b7f))


### Bug Fixes

* **mobile:** 手机端的平台识别为 Android ([#44](https://github.com/sunerpy/lockra/issues/44)) ([61473c6](https://github.com/sunerpy/lockra/commit/61473c637ebdf941288d02db0e6eb7ca8568ca7d))

## [0.6.0](https://github.com/sunerpy/lockra/compare/v0.5.1...v0.6.0) (2026-10-02)


### Features

* **ui:** 批量修改账号分组，Touch ID / Windows Hello 可直接开启 ([#27](https://github.com/sunerpy/lockra/issues/27)) ([9c6ae1e](https://github.com/sunerpy/lockra/commit/9c6ae1e815032e2f0c80d01abe025b60527b2a9e))

## [0.5.1](https://github.com/sunerpy/lockra/compare/v0.5.0...v0.5.1) (2026-10-02)


### Bug Fixes

* **core:** 合并恢复备份与替换密钥时保留账号的颜色与头像文字 ([#25](https://github.com/sunerpy/lockra/issues/25)) ([a2752d4](https://github.com/sunerpy/lockra/commit/a2752d4627f53ed6ec874cae432094e7bb461d15))

## [0.5.0](https://github.com/sunerpy/lockra/compare/v0.4.0...v0.5.0) (2026-10-02)


### Features

* **ui:** 增加分组折叠、账号颜色与 Touch ID / Windows Hello 解锁 ([#22](https://github.com/sunerpy/lockra/issues/22)) ([a6f18dd](https://github.com/sunerpy/lockra/commit/a6f18dd03988167161ac91f9151d6da5ff63716b))

## [0.4.0](https://github.com/sunerpy/lockra/compare/v0.3.2...v0.4.0) (2026-10-02)


### Features

* **sync:** 增加端到端加密的多设备同步 ([#19](https://github.com/sunerpy/lockra/issues/19)) ([ec9cf4d](https://github.com/sunerpy/lockra/commit/ec9cf4d5998667306e9474b2ac2b3ce4a3f91176))

## [0.3.2](https://github.com/sunerpy/lockra/compare/v0.3.1...v0.3.2) (2026-10-01)


### Bug Fixes

* **update:** 0.3.2 之前保存的自动更新开关一律关闭，需重新开启 ([#17](https://github.com/sunerpy/lockra/issues/17)) ([ab255a3](https://github.com/sunerpy/lockra/commit/ab255a3db22b2d4bff0d36c20eb47f21c422ce14))

## [0.3.1](https://github.com/sunerpy/lockra/compare/v0.3.0...v0.3.1) (2026-10-01)


### Bug Fixes

* **update:** 不再把 0.2.0 的自动检查开关沿用为自动更新 ([#15](https://github.com/sunerpy/lockra/issues/15)) ([54eb1de](https://github.com/sunerpy/lockra/commit/54eb1deacc10de4b70018011a4d593fa9f172fcf))

## [0.3.0](https://github.com/sunerpy/lockra/compare/v0.2.0...v0.3.0) (2026-10-01)


### Features

* **update:** 照 Voltip 的设计重做应用内更新与升级界面 ([#12](https://github.com/sunerpy/lockra/issues/12)) ([a34b8c1](https://github.com/sunerpy/lockra/commit/a34b8c11f5fbabf917e0a2209cf914ab9e76e515))

## [0.2.0](https://github.com/sunerpy/lockra/compare/v0.1.1...v0.2.0) (2026-10-01)


### Features

* **update:** 增加应用内更新与各平台一键安装脚本 ([#9](https://github.com/sunerpy/lockra/issues/9)) ([bd84c36](https://github.com/sunerpy/lockra/commit/bd84c36a973855c0e9d897f0770ac1fe321f33e5))

## [0.1.1](https://github.com/sunerpy/lockra/compare/v0.1.0...v0.1.1) (2026-10-01)


### Bug Fixes

* **desktop:** Windows 原生构建不再重复链接 UCRT ([#5](https://github.com/sunerpy/lockra/issues/5)) ([d15b801](https://github.com/sunerpy/lockra/commit/d15b801cfa8997bb6de01d06990827313cae95c6))

## 0.1.0 (2026-10-01)


### Features

* **app:** 初始化 Lockra 桌面两步验证器 ([f5f1c1d](https://github.com/sunerpy/lockra/commit/f5f1c1dc3caa1e3d127be4c95cf09fc7f062819e))
