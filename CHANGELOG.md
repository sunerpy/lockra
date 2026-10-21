# Changelog

## [0.16.1](https://github.com/sunerpy/lockra/compare/v0.16.0...v0.16.1) (2026-10-21)


### Bug Fixes

* **deps:** 将 rustls-webpki 更新到 0.103.16，修复 RUSTSEC-2026-0382 ([#196](https://github.com/sunerpy/lockra/issues/196)) ([f5983c1](https://github.com/sunerpy/lockra/commit/f5983c10b2a33697b36040e890869cef51998a4c))

## [0.16.0](https://github.com/sunerpy/lockra/compare/v0.15.0...v0.16.0) (2026-10-20)


### Features

* **extension:** 识别隐藏输入的验证码框，支持按完整域名匹配与关联验证码 ([#191](https://github.com/sunerpy/lockra/issues/191)) ([e490a0a](https://github.com/sunerpy/lockra/commit/e490a0ac1608ceff556996e2a91bb2c213f9f786))

## [0.15.0](https://github.com/sunerpy/lockra/compare/v0.14.0...v0.15.0) (2026-10-19)


### Features

* **relay:** 新增注册码模式，新的同步空间凭管理员生成的一次性注册码登记 ([#188](https://github.com/sunerpy/lockra/issues/188)) ([ccbecf9](https://github.com/sunerpy/lockra/commit/ccbecf936c11802a0daec25e1c367c1b85f3d141))

## [0.14.0](https://github.com/sunerpy/lockra/compare/v0.13.1...v0.14.0) (2026-10-19)


### Features

* **relay:** 增加私有空间准入并移除内置中继 ([342b680](https://github.com/sunerpy/lockra/commit/342b6800efee86f51b65786937d79eb38391dedf))

## [0.13.1](https://github.com/sunerpy/lockra/compare/v0.13.0...v0.13.1) (2026-10-18)


### Bug Fixes

* **extension:** 网页自动聚焦的登录输入框也弹出菜单 ([#183](https://github.com/sunerpy/lockra/issues/183)) ([a069eee](https://github.com/sunerpy/lockra/commit/a069eee2ad17ff140e3339c9615e0fcfde1b8533))

## [0.13.0](https://github.com/sunerpy/lockra/compare/v0.12.0...v0.13.0) (2026-10-18)


### Features

* **desktop:** 新增「登录时启动 Lockra」，用系统自带的启动项，后台运行时只留在托盘 ([#181](https://github.com/sunerpy/lockra/issues/181)) ([8f7756a](https://github.com/sunerpy/lockra/commit/8f7756a5ec49fc320584628809b314fb6ea6e189))


### Bug Fixes

* **mobile:** 离开应用时由原生层锁定保险库，不再只靠 WebView 的页面事件 ([#179](https://github.com/sunerpy/lockra/issues/179)) ([9540432](https://github.com/sunerpy/lockra/commit/9540432b572e27df0ba01c3e5fde60ac8b39dfd2))

## [0.12.0](https://github.com/sunerpy/lockra/compare/v0.11.1...v0.12.0) (2026-10-17)


### Features

* **extension:** 新增「匹配端口」开关，同一地址不同端口的登录项分开填充 ([#174](https://github.com/sunerpy/lockra/issues/174)) ([d41ff9e](https://github.com/sunerpy/lockra/commit/d41ff9e186d49f79dd8f1f12be4a59aedded91d1))
* **extension:** 菜单和弹窗可以删除登录项，弹窗可以填入和复制验证码 ([#177](https://github.com/sunerpy/lockra/issues/177)) ([d402902](https://github.com/sunerpy/lockra/commit/d4029024a34ee220957d0b95d47e5837ab9a9a29))
* **extension:** 页内菜单匹配到多个登录项时可以搜索 ([#176](https://github.com/sunerpy/lockra/issues/176)) ([1de7fa9](https://github.com/sunerpy/lockra/commit/1de7fa935e430a2a75d2a2429e280930c0f47223))


### Bug Fixes

* 识别 Google 身份验证器的验证码框，详情栏可直接关联验证码 ([#172](https://github.com/sunerpy/lockra/issues/172)) ([6ec3726](https://github.com/sunerpy/lockra/commit/6ec37260c6e1f50d35170db86bfe3b2f30c28f9a))

## [0.11.1](https://github.com/sunerpy/lockra/compare/v0.11.0...v0.11.1) (2026-10-17)


### Bug Fixes

* **extension:** 改名只差大小写的模块，修复 macOS 和 Windows 上的发布构建 ([#169](https://github.com/sunerpy/lockra/issues/169)) ([a307e13](https://github.com/sunerpy/lockra/commit/a307e13415f01c622bbaabd724b81058270f1b8d))

## [0.11.0](https://github.com/sunerpy/lockra/compare/v0.10.1...v0.11.0) (2026-10-17)


### Features

* **extension:** 扩展随 Lockra 自动更新，版本落后时在弹窗和设置里提示 ([#166](https://github.com/sunerpy/lockra/issues/166)) ([27fae8a](https://github.com/sunerpy/lockra/commit/27fae8ab134b5bab2c76ecabeb72cdc08fe6e6fe))
* **sync:** 开始同步时默认用自己的存储，推荐自建中继，并写明外泄后的处理 ([#167](https://github.com/sunerpy/lockra/issues/167)) ([cc87809](https://github.com/sunerpy/lockra/commit/cc87809400425da33470e87b7acefcbefb6fdce4))

## [0.10.1](https://github.com/sunerpy/lockra/compare/v0.10.0...v0.10.1) (2026-10-17)


### Bug Fixes

* **extension:** 同一表单只生成一个密码，确认密码框不再换新密码，并可复制生成的密码 ([#161](https://github.com/sunerpy/lockra/issues/161)) ([d4e3e14](https://github.com/sunerpy/lockra/commit/d4e3e14fcbcbf2d1edbd2794f3d37de15101836e))
* **ui:** 命令面板刚打开时按 Esc 也能关闭，焦点不在面板里时按键照样生效 ([#165](https://github.com/sunerpy/lockra/issues/165)) ([c072ff3](https://github.com/sunerpy/lockra/commit/c072ff3321819d0a267ea84d3fa84de9b3be8455))
* **ui:** 编辑登录项后详情栏的验证码不再停住 ([#163](https://github.com/sunerpy/lockra/issues/163)) ([735c2d3](https://github.com/sunerpy/lockra/commit/735c2d3e4948b5f7c4778382c6b8eed19c8334db))

## [0.10.0](https://github.com/sunerpy/lockra/compare/v0.9.1...v0.10.0) (2026-10-16)


### Features

* **autofill:** 新增网址匹配、密码规则与生成器 ([#126](https://github.com/sunerpy/lockra/issues/126)) ([6515ab2](https://github.com/sunerpy/lockra/commit/6515ab2ee514718f079b980c50dae2bf8c9dbda9))
* **browser:** 新增扩展与应用之间的加密通道和配对 ([#130](https://github.com/sunerpy/lockra/issues/130)) ([7124dbe](https://github.com/sunerpy/lockra/commit/7124dbee5dea74265aed85e758a1a7cf1941f610))
* **core:** 在保险库中保存登录项并按字段合并 ([#128](https://github.com/sunerpy/lockra/issues/128)) ([7a0021e](https://github.com/sunerpy/lockra/commit/7a0021e78199a884fce257baec5640b7402a15fd))
* **core:** 增加登录项与生成器命令 ([#129](https://github.com/sunerpy/lockra/issues/129)) ([86f2f2c](https://github.com/sunerpy/lockra/commit/86f2f2cb1aebaef64ac03d103e6d334a461415ce))
* **desktop:** 关联验证码时可当场新建账号，行菜单可移到分组 ([#146](https://github.com/sunerpy/lockra/issues/146)) ([6e19116](https://github.com/sunerpy/lockra/commit/6e191163b7e750c6654a62f51aec45f116bb81c2))
* **desktop:** 托盘图标常驻，右键菜单增加设置与检查更新 ([#148](https://github.com/sunerpy/lockra/issues/148)) ([a641a47](https://github.com/sunerpy/lockra/commit/a641a4718ff3ce44712ec49465836d44b58bdf2d))
* **desktop:** 新增「设置 › 浏览器」、配对批准与填充确认 ([#138](https://github.com/sunerpy/lockra/issues/138)) ([7d492e1](https://github.com/sunerpy/lockra/commit/7d492e1f11262e35d0fe4a2e3b6b2b5a1ff23ae1))
* **desktop:** 新增密码页面、生成器和面板次级操作 ([#132](https://github.com/sunerpy/lockra/issues/132)) ([f12f3b0](https://github.com/sunerpy/lockra/commit/f12f3b0be856da95ea7ac0c858d0692606aff04c))
* **desktop:** 新增浏览器扩展的宿主模式、本地监听与后台运行 ([#136](https://github.com/sunerpy/lockra/issues/136)) ([717e02d](https://github.com/sunerpy/lockra/commit/717e02d7aa9a10b0d44818c8ae3e054bd4c2d95a))
* **desktop:** 用全局快捷键随处打开命令面板 ([#143](https://github.com/sunerpy/lockra/issues/143)) ([5c8670c](https://github.com/sunerpy/lockra/commit/5c8670c4daf938c921f453595a9d2d9ecf86afeb))
* **export:** 把选中的登录项导出为 KDBX 或 CSV ([#137](https://github.com/sunerpy/lockra/issues/137)) ([136c510](https://github.com/sunerpy/lockra/commit/136c510fdecb0cccbe6f79706acd6cce049d7378))
* **extension:** Lockra 菜单开在输入框上方，避开浏览器自己的密码列表 ([#158](https://github.com/sunerpy/lockra/issues/158)) ([da2236c](https://github.com/sunerpy/lockra/commit/da2236cd6e6e5bab10348a762b96ae1fc7554877))
* **extension:** 分几页的登录自动接着填密码，关联验证码的登录项填入验证码后才提交 ([#155](https://github.com/sunerpy/lockra/issues/155)) ([2c17c0c](https://github.com/sunerpy/lockra/commit/2c17c0cdc8954452631ea65ccf0e597da1b10c84))
* **extension:** 可接管浏览器自带的密码功能，自动提交只在填入密码后进行 ([#154](https://github.com/sunerpy/lockra/issues/154)) ([6447e77](https://github.com/sunerpy/lockra/commit/6447e778c390e7cfb632e3e48bec0a064a795656))
* **extension:** 新增 Chrome、Edge 与 Firefox 扩展 ([#135](https://github.com/sunerpy/lockra/issues/135)) ([2b62359](https://github.com/sunerpy/lockra/commit/2b62359e5cca28342610f9284fc877738e99fa8b))
* **import:** 导入浏览器和密码管理器的导出文件 ([#134](https://github.com/sunerpy/lockra/issues/134)) ([bbb5b3e](https://github.com/sunerpy/lockra/commit/bbb5b3ebcbc24f320dab35bc129d8b59cac83401))
* **mobile:** 新增手机端的密码列表、登录项、编辑和显示密码 ([#133](https://github.com/sunerpy/lockra/issues/133)) ([ba70037](https://github.com/sunerpy/lockra/commit/ba700370cc87ca97325346453c549386e11392c9))
* **sync:** 同步设置标出看不到密码的旧设备，快照接近中继上限时提示 ([#152](https://github.com/sunerpy/lockra/issues/152)) ([c8d80d7](https://github.com/sunerpy/lockra/commit/c8d80d7644fd3cc1919519a0ad8d91f87f822c4c))
* 开放密码管理与浏览器自动填充 ([#149](https://github.com/sunerpy/lockra/issues/149)) ([6b121f0](https://github.com/sunerpy/lockra/commit/6b121f029e980ce8cee52466161090dc9d90b58e))


### Bug Fixes

* **deps:** 升级 rustls 到 0.23.46，修复四个安全公告 ([#131](https://github.com/sunerpy/lockra/issues/131)) ([57f0250](https://github.com/sunerpy/lockra/commit/57f0250e5d25cd767ad1e359241910b09cc52a60))
* **extension:** 内联菜单和提示框的阴影改在框架外绘制 ([#142](https://github.com/sunerpy/lockra/issues/142)) ([23b47b8](https://github.com/sunerpy/lockra/commit/23b47b89b04ed919df36fcbe2f8505e78bb8d299))
* **sync:** 中继拒收过大的快照时说明原因并给出大小 ([#153](https://github.com/sunerpy/lockra/issues/153)) ([d13c6d3](https://github.com/sunerpy/lockra/commit/d13c6d38c88401e3ff79155a531795678a1c8743))
* **ui:** 焦点在条目按钮上时不再拦下全局快捷键 ([#150](https://github.com/sunerpy/lockra/issues/150)) ([dde08a7](https://github.com/sunerpy/lockra/commit/dde08a7be7adb3b0a05b044a0677bb7db4c7a2be))
* **ui:** 统一「收藏」与「关闭窗口后继续运行」的英文用词 ([#147](https://github.com/sunerpy/lockra/issues/147)) ([9beea95](https://github.com/sunerpy/lockra/commit/9beea9587c118b496203ea6ee9c8d38c0c2a0374))

## [0.9.1](https://github.com/sunerpy/lockra/compare/v0.9.0...v0.9.1) (2026-10-11)


### Features

* 新安装默认使用澄空主题 ([#120](https://github.com/sunerpy/lockra/issues/120)) ([be19662](https://github.com/sunerpy/lockra/commit/be19662b8958a160daa7341c7483536b44e6fdc2))

## 0.9.0 (2026-10-11)


### Features

* Lockra 初始版本 ([c764dc0](https://github.com/sunerpy/lockra/commit/c764dc0e20d3cea92ae0fb31528fda4e4d4c0071))

## Changelog
