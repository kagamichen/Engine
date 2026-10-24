# 鸿蒙产品宿主绑定补齐

状态：开发集成完成，未完成发布及设备验收。

## 完整负责人

Engine 的已有 Operation 拥有历史、搜索、剪贴板去重、发送、恢复和成员规则。N-API 只接收完整操作参数，调用一个 Operation，映射稳定结果；ArkTS 不访问内部 crate 或数据库。绑定不新增持久状态、加密格式或重试队列。

## 问题

当前 index.d.ts 与 runtime.rs 漂移：未声明 sendImage/sendFiles/captureCurrentClipboard/restoreClipboard/chooseDeviceGroup；已退役的查询仍在声明和验收宿主出现。另外 remove_member 仍匹配旧 WorkspaceMembership，但当前 Operation 返回 DeviceTrust，会把成功移除误报为失败；同步修正该边界映射。产品宿主需要历史搜索、设备列表、自动剪贴板变化观察等公开操作。

## 实施与验收

- [x] 同步 N-API 方法、事件和结果声明，删除已不存在的方法及宿主调用。
- [x] 提供类型化分页历史/搜索、设备列表和观察剪贴板的薄绑定。
- [x] 用真实 Node N-API 加载模块比对声明方法，测试正文 round-trip、分页、搜索、恢复和错误。
- [x] 运行针对性 Rust 测试和格式检查；记录基线或环境故障。
- [x] 补齐历史正文/图片资源读取、删除与重新发送，接收进度/取消及事件负载，保持一个绑定方法对应一个既有 Operation。
- [x] 开发 HAR 构建清单与产品显式导入/恢复官方依赖，9 项清单测试与 11 项导入测试通过。
- [x] 异步剪贴板 Host 回调支持与真实 N-API 文字/PNG、拒绝/超时/晚到验证；产品已接入 ImageKit/Pasteboard 适配，真实设备图片验收仍待完成。
- [x] 补齐首次设置/加密状态、显式解锁/锁定、恢复状态、全局同步偏好及变更事件；验证会话状态、口令恢复和设置重启保持。
- [x] API 24+ 编译 HAR/HAP，并按同版本产物规则更新产品依赖（API 26 SDK）。
- [ ] 真机与桌面配对、双向内容、断网恢复和安全存储验收。

成功结果来自 OperationResult。EngineError 继续映射现有稳定 UC_ENGINE 错误码，不泄漏内容。暂停/关闭/取消由 Engine 生命周期控制，业务恢复依原有 Engine 流程，宿主不自行重发历史。

历史与传输扩展仍由既有 Engine Operation 负责；绑定不读取数据库、拼接网络流程或缓存内容。字节计数跨 JavaScript 边界使用十进制字符串，取消保留 attempt_id 防止取消新一次接收；事件负载映射既有结果，不新增流程查询。取消的成功、过晚、已结束和已被替代分别呈现，重试仅由用户显式重新发送触发。


## 验证记录

- Rust 1.95：`cargo build -p uc-ohos-napi --locked` 成功；`cargo test -p uc-ohos-napi --locked` 54 项通过。public_contract 原有未使用导入警告仍在。
- `cargo metadata --locked --format-version 1`、`cargo check --workspace --all-targets --locked`、`cargo fmt --all -- --check`、Rust 编写规范检查、Engine 架构预检通过。全工作区检查另有既存 golden_vectors 未使用导入警告。
- `host_smoke.cjs`：真实模块方法与声明一致，文字/图片/文件 round-trip，分页/搜索、统一观察索引和去重、前后台、重启与口令/文字明文哨兵扫描通过。
- `host_failure_smoke.cjs`：存储不可用启动失败与日志目录不可用能力降级断言通过。
- `p2p_smoke.cjs`：两个独立进程真实配对，双向文字、PNG 字节、256 KiB 文件导出比对、同步偏好、回环抑制和移除成员通过。生产 Iroh 节点有进程唯一租约，不能在一个进程启动两个 Engine 代替双设备。
- 全局同步控制：在已配对的两个进程中分别关闭 syncEnabled / autoSyncEnabled，观察操作保留本地历史且不改变对端历史/剪贴板；重新开启后新复制成功到达对端。当前 Engine 将已采集但未分发映射为 UC_ENGINE:1253:conflict:false，宿主不能把该结果解释为已发送。
- 以上宿主运行于 macOS，系统回调为内存测试宿主；不是鸿蒙设备、跨网或产品 HAR 的证明。
- 旧 captureCurrentClipboard 仅保存，不负责实时索引。新增观察接口调用统一 ObserveClipboardChange，后者拥有索引、去重和分发完整流程；测试对两条入口分别验证，不在绑定或产品补第二套索引。
- 产品仓已消费经清单验证的开发 HAR；API 26 SDK 下 Engine HAR 与产品未签名 HAP 构建通过。真机及发布仍待完成。

复现（仓库根目录，macOS）：

```bash
cargo build -p uc-ohos-napi --locked
cp target/debug/libuc_ohos_napi.dylib target/debug/uc_ohos_napi.node
UC_OHOS_NAPI_NODE="$PWD/target/debug/uc_ohos_napi.node" node bindings/uc-ohos-napi/tests/host_smoke.cjs
UC_OHOS_NAPI_NODE="$PWD/target/debug/uc_ohos_napi.node" node bindings/uc-ohos-napi/tests/host_failure_smoke.cjs
UC_OHOS_NAPI_NODE="$PWD/target/debug/uc_ohos_napi.node" node bindings/uc-ohos-napi/tests/p2p_smoke.cjs
```

## 开发包构建与消费

```bash
bash tests/hosts/ohos/build-emulator.sh --har-only
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s bindings/uc-ohos-napi/tests -p 'test_*.py' -v
```

`--har-only` 生成 HAR 与同目录 `.build.json`，跳过验收 HAP 和签名，复用仓库 target。清单记录实际源码指纹、SDK、包/声明/原生库哈希；对照打包原生库和方法集合，源码中途变化则失败。SDK 不足时在安装依赖或 Cargo 编译前退出；API 26 构建现已通过。

消费仓通过 `prepare-engine.py --local-har` 显式导入，默认仍固定官方发布包。开发清单不构成发布认证；声明集合检查不是完整签名兼容证明。API 26 HAR/HAP 已通过，设备矩阵和发布签名仍待完成。

历史详情/资源/删除/重发及接收取消已通过真实模块验证；双独立进程验证了传输事件含方向和十进制字节计数。未把 macOS 原生验证视为鸿蒙设备证明。

## 2026-10-05 集成结果

- Hvigor 会对 `.so` 执行 strip；清单校验已改为分别记录源库和 HAR 库 SHA-256，并检查非空 AArch64 ELF。源库与 HAR 库不再要求字节相同；清单测试 9 项通过。
- Engine API 26 HAR 构建通过，产品导入同一 HAR 后 API 26 `assembleHap` 通过，生成未签名 HAP。
- 产品会话层已接入新版历史/设备/同步/恢复/文件接口；该计划仍不把 HAP 构建当作真机配对、权限、后台、网络或发布验收。

## 图片异步 Host 绑定

既有 Engine 剪贴板端口仍拥有采集/恢复流程。平台适配需要在 JS 线程发起图片异步编码，并仅把完成后的独立数据送回等待的原生工作线程。不得把 JsObject 跨线程传递、阻塞 JS 线程、伪造同步编码或建立陈旧剪贴板缓存。绑定已统一同步结果与 Promise 的完成/拒绝/超时处理。JS 对象、属性检查和 Uint8Array 复制留在 JS 线程，原生工作线程只接收独立数据；晚到结果不再读取其正文。回调闭包由 N-API finalizer 管理，没有每次调用额外启动的 Rust 等待任务。产品只在新 HAR 验证后接入。

真实 N-API 测试已通过异步文字/PNG 采集和恢复、Promise 拒绝、权限拒绝、null 结果、抛错属性、5 秒超时、晚到结果不读取正文、永不完成后继续读取及生命周期关闭。拒绝正文哨兵未出现在持久文件中。测试等待生命周期事件改为时间期限，不再把队列中前 50 个事件当作生命周期到达期限。

5 秒期限只限制 Engine 等待，并不取消已经开始的系统异步操作，尤其不能撤销可能已执行的剪贴板写入。宿主仍须正确管理系统资源与写入完成语义；大图内存、编码耗时和关闭期间的实际行为必须真机验证。

## 设置与安全会话补齐

既有 Engine Operation 唯一拥有设置持久化、合法性判断、解锁、锁定和会话恢复。N-API 只映射 QuerySetupState / QueryEncryptionState / UnlockSpace / LockEncryption，以及 QuerySettings / UpdateSettings 中的同步分区。每个调用只执行一个 Operation；更新结果保留业务拒绝，不隐式查询或恢复会话。口令在 Rust 入口用 Zeroizing/SecretString 接管，产品不存储口令。恢复是否完成通过 Engine 的状态和 RecoverSession 结果判断，不从设备名或空历史猜测。

QueryProfileRecovery 与 ProfileRecoveryChanged 保留 Engine 的恢复阶段、口令提交许可、重启要求、损失分类及 admission 恢复动作。不能把凭据不匹配或需恢复备份的状态统一显示成“再次输入口令”。SettingsChanged 只携带变更分区，不在事件中复制设置值。

真实 N-API 单宿主已验证首次设置状态、邀请查询、部分设置更新不覆盖未指定字段、非法频率拒绝、设置事件及重启保持。锁定后会话状态为未就绪，RecoverSession(false) 不自动解锁；错误口令失败，正确口令恢复同一空间和历史。

清空替身安全存储并重启后，Engine 进入 awaiting_passphrase，历史查询与详情不可访问；错误口令不写入新凭据，正确口令恢复原有空间、历史与设置，恢复事件与查询一致。该证据来自内存 Host，不代表鸿蒙 Asset Store 真机恢复已通过。

LockEncryption 不构成完整的 GUI 内容授权边界：历史维护读取可以使用临时加载的密钥，已有部分搜索/详情路径仍可读。界面隐私锁须遵循[安全会话与 GUI 授权的区别](../../security/encrypted-persistence.md)，不能声称调用 LockEncryption 就阻止了所有内容读取。本次未改变核心锁定语义。

同步开关验证针对 ObserveClipboardChange；不扩张为所有显式重发路径的保证。ResendEntry 的空目标先返回 no_eligible_targets，不能用无目标的单宿主测试验证停发。双进程开关测试放在本机首次接收之前，避免既有远端写入的回环抑制窗口吞掉模拟本地复制，使测试误判开关行为。


## API 26 图片输入类型回归

真实 ArkUI 客户端的 ImageKit 编码成功，但 sendImage 的 Rust Buffer 参数不能接受鸿蒙 Uint8Array。入口改用 Uint8Array，与公开声明一致，仍只调用一个 SendImage Operation；成功、失败及重试由既有 Engine 流程拥有。未改变协议或持久化格式，也不在客户端伪造 Node Buffer。

Host 烟测使用带偏移的普通 Uint8Array，并核对历史资源字节。Node 对旧 Buffer 入口的宽松接受无法复现鸿蒙问题；API 26 上新 HAR 已消除参数错误，支持图片历史保存和实际预览。双模拟器重启后互报离线，新文本同样离线，双端图片接收仍未通过。重启恢复需要独立排障。

Rust 1.95.0：54 项绑定测试、真实 Host 烟测、workspace all-targets 检查、格式、metadata、规范和架构预检通过。API 26 HAR 与客户端 Release HAP 打包、测试签名及审计通过。构建脚本优先使用 Cargo 用户 bin 目录，确保 rustup 按 rust-toolchain.toml 选择已安装鸿蒙目标的版本，避免 PATH 优先命中 Homebrew Cargo。

## 手动发送与自动同步开关

双模拟器验证暴露出显式 SendImage 被自动同步开关拦截；真实 N-API 也复现了 SendText 返回 1253。本机保存成功不代表出站成功。完整负责人仍为 Application 的 LocalClipboardProcessor 与 clipboard sync 模块，调用方只执行 SendText / SendImage / SendFiles。内部 dispatch 必须携带自动或显式发送意图：全局同步关闭阻止二者，autoSyncEnabled 只阻止自动同步。内容类型、成员、目标选择、停止和投递序列化仍沿既有流程判断。离线恢复仍属于自动动作，遵守自动同步开关；不新增后台重试策略、绑定接口或持久化格式。成功保留各目标回执；跳过或失败沿现有错误链返回，用户可重试。

验收：验证设置矩阵、意图和目标传递、停止门禁，以及真实文字/图片/文件显式发送。重新生成 HAR/HAP 后，关闭发送端自动同步并确认接收端无新记录，再手动发送新图片核对接收和预览。

结果：460 项 Application 剪贴板测试通过；真实 N-API 单宿主与双独立进程验证自动同步关闭下的手动文字/图片/文件、对端字节比对及全局关闭拒绝。workspace all-targets、metadata、fmt、规范与架构预检通过，开发清单 9 项测试通过。重新生成 API 26 HAR，产品导入并完成 Release 模拟器签名构建。两台模拟器自动同步均关闭，B 复制新双色图片时 A 无新记录；确认在线后单次手动发送回执 accepted=1、offline=0、error=0，A 新增记录且实际预览正确。该结果修正上文“手动图片接收尚未通过”的历史状态；不代表所有网络、HDR、大图或真机矩阵通过。

## 空文件导入与传输

真实 N-API 已复现零字节 Host 文件在 SendFiles 导入前返回 1251。Host 输入适配唯一负责按元数据读取完整文件；文件长度为零仍是有效文件，不等于没有选中文件。删除仅针对零长度的拒绝，保留空句柄列表、非法 basename、读取截断和取消判断。Application 继续唯一负责采集、历史、分发和恢复，不新增协议或持久化格式。调用方仍只执行 SendFiles 或 ExportEntry，成功以发送回执和最终导出结果判断，失败沿原错误链返回并由用户重试。先运行真实单宿主空文件回归，再验证双进程到达、零长度导出，以及更大文件和多文件范围。

结果：失败回归转为通过，真实单宿主完成空文件发送/导出；双独立进程完成零字节文件到达/导出和 8 MiB 随机内容文件字节比对。workspace all-targets、metadata、fmt、Rust 规范、架构预检及 API 26 HAR/HAP 构建通过。模拟器 B 系统 Picker 选取零字节文件后首次回执离线；接收端恢复后出现该文件，系统导出为 0 字节且 SHA-256 与原文件一致。没有把离线回执记为成功，也没有把原生测试扩张为大文件真机验收。

多文件缺口：ResourceFacade::entry_file 明确读取第一个本地文件，ExportEntry 当前复用该入口，因此不能导出完整文件集合。后续须为用户提供完整集合导出或逐项选择导出的完整能力，保留名称并验证全部文件；不得仅在产品中隐藏多选以宣称 M6 完成。

## 完整文件集合导出

新增显式 ExportEntryFiles 动作，将文件集合写为 ZIP；旧 ExportEntry/ReadEntryFile 保持兼容。完整负责人是 Application 的 clipboard resource 文件导出模块：从历史解析全部来源和显示名称、检查可用性、分块归档、完成目标。Engine 只把 Host 句柄适配为可定位写入的目标，N-API 和产品只调用一次完整动作。UI 明确显示“导出 ZIP”，使用系统保存窗口。正常名称保留，同名及不安全名称由唯一归档命名规则处理，不跳过文件。没有新增内部明文暂存包或持久化格式。

成功要求全部文件写入且宿主 finish_write 成功；任一来源缺失、读取/写入失败、取消均失败，不报告部分包成功。输出文件由用户选择，失败可留下部分内容，重试重新选择目标。文件内容按固定块读取，ZIP 直接写目标，不把文件集合全部缓存在内存。验收包括独立 ZIP 解包器逐项比对、空文件、中文/空格、同名、危险名称、缺失来源和目标失败，以及真实 N-API 与 ArkTS 构建。


### N-API 启动参数所有权

完整集合双进程回归连续两次在启动时 SIGSEGV，栈位于安全存储 ThreadsafeFunction 的句柄读取，尚未执行文件发送。`start_engine` 原来是异步绑定，直接持有仅包裹 JS 外部指针的 `External<PreparedHost>`；napi 2 的 External 转换不会为异步任务建立所有权。改为 `ConsumedHost` 参数转换在 JS 调用期间取走 Host，再把拥有值交给异步入口；生命周期类引用由 napi 异步宏建立引用保护。JS 仍收到 Promise，Engine 启动/失败/清理由原负责人完成。不以测试长期保留 JS 变量掩盖绑定生命周期问题。验证临时参数、主动 GC、重复消费拒绝和真实双进程启动及同步。


归档验证结果：7 项 Application、1 项真实文件读取适配器、4 项 Engine 目标写入测试通过。真实单宿主三文件 ZIP 经 Python 独立解包逐字节比对通过；修复启动所有权后，主动 GC 的双独立进程完成 8 MiB 随机内容、同名第二文件与空文件的完整集合传输/ZIP 导出，独立 Python 解包及 SHA-256 逐项比对通过。该验证来自 macOS Host，不替代鸿蒙系统 Picker 或真机矩阵。产品 81 项 Node、11 项产物校验测试通过。

## API 26 二进制输出类型修复

模拟器历史搜索成功，但纯文本恢复在 Host 解码阶段返回 401。临时诊断确认绑定回调的 bytes 是 ArrayBuffer 而非公开声明中的 Uint8Array；napi 的 Buffer 在鸿蒙与 Node 上具有不同的运行时表示。Node 旧测试把任意输入复制为 Buffer，掩盖了契约漂移。

完整负责人是 N-API 绑定层：安全存储写入、文件块写入、剪贴板快照、历史内联数据和 blob 结果统一输出 Uint8Array。宿主继续直接消费声明类型，不增加 ArrayBuffer/Buffer 兼容分支；Engine 业务、持久化、网络和重试责任不变。调用方仍只调用既有完整操作，成功以系统恢复后的完整正文核对为准，失败继续沿既有结果返回。

先增加真实原生模块输出类型回归，已在旧模块上复现 Buffer 类型断言失败。待原生检查、API 26 HAR/HAP 重建和模拟器实际恢复验证后记录结果；本项不替代真机及全矩阵验收。


输出类型修复结果：绑定 Rust 54 项、真实 N-API 单宿主和双独立进程、workspace all-targets、fmt、metadata、Rust 规范与架构预检通过；API 26 HAR/HAP 重建并校验成功。鸿蒙 A 上“历史恢复 → 读取剪贴板”完整正文一致，删除后的搜索及进程重启保持通过；同包系统导出的四文件 ZIP 独立逐项哈希比对通过。临时产品日志探针已移除。双模拟器新包互通、真机和完整发布矩阵仍待执行，不据此关闭本计划。

## 上游 main 合并（rc.23）

合并 origin/main 的 87 项提交：上游将 uc-infra 拆分为七个 infra crate，网络栈迁入 third_party/iroh 子模块。文件导出读取适配器随 fs 能力迁入 uc-infra-local，并为端口实现补充 uc-application 依赖（与其他 infra crate 的端口实现方向一致）。客户端契约的四个传输方法保留 transfer.rs 的 Operation 通道版本，删除上游重复实现与含 items 的 OhReceiveProgress/OhReceiveItem/OhWorkspaceConvergence；保留上游独有的 retryEntryReceive 与 OhReceiveRetryOutcome。恢复流程采用上游 RecoveryOutcome 结构，门控沿用显式意图 delivery_enabled(Automatic)，补 CancellationIncomplete 结果分支，移除上游已删除的设置分区。声明与烟测对齐合并后的 51 方法面（版本 rc.23），静态比对实现与声明零漂移。

验证：workspace all-targets、uc-ohos-napi 54 项、绑定清单 Python 9 项、fmt 通过。注意：Rust 规范与架构门禁在 origin/main 即为红（local_file.rs、network_log.rs 等为上游原有违规，文件与上游逐字节一致），非本次合并引入；正式发布前需上游修复或明确豁免。W1.1 的“方法在主线可用”部分就此成立，发布（W1.2）、真实 N-API 烟测与设备验收仍待执行。
