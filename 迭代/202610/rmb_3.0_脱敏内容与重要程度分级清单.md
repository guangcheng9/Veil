# rmb_3.0 脱敏内容与重要程度分级清单

> 整理日期：2026-10-10  
> 面向：项目开发、运维、安全维护人员及 Veil 维护人员  
> 目标项目：`D:\workspace\cnb\rmb_3.0`  
> 配套方案：[rmb_3.0_Veil项目专属脱敏配置方案](./rmb_3.0_Veil项目专属脱敏配置方案.md)  
> 文档性质：配置与数据分类清单，不是已经生效的配置包或完整安全审计报告

## 1. 如何使用这份清单

配套方案回答“Veil 需要怎样适配”；本文回答“rmb_3.0 具体有哪些内容需要脱敏，哪些应当优先处理”。

将敏感内容分成四档：

| 等级 | 重要程度 | 判断依据 | 建议默认策略 |
|---|---|---|---|
| S0 | 必须脱敏：凭据与控制能力 | 可以用于认证、签名、解密、访问资源或获得会话权限 | 向外部模型发送时始终遮盖实际值 |
| S1 | 高优先级：个人与资金数据 | 能识别自然人、关联账户、暴露资金关系或业务机密 | 发送真实数据前默认脱敏，按授权范围保留必要信息 |
| S2 | 按场景脱敏：企业与基础设施信息 | 暴露企业归属、环境结构、部署位置或内部命名，但通常不能单独获得访问权限 | 对外分享开启；内部代码排障按需要保留 |
| S3 | 通常保留：配置结构与非敏感元数据 | 字段名、类型、框架配置、公共接口结构等 | 保留以帮助模型理解问题，避免过度脱敏 |

**这里的 S0–S3 是信息风险等级，不是原方案中 P0/P1/P2 的开发优先级，也不是 Veil 正则规则的数字 priority。** 高风险内容不一定要用更高数字的正则先匹配；还需考虑完整私钥、连接串与内部片段的重叠关系。

### 1.1 核实范围与证据边界

- 综合既有项目抽查、配套适配方案及本次代表性配置复核。
- 范围包含 `backend/`、`frontend/`，以及必要的 `test/`、`devops/` 和构建配置。
- 文件路径均相对于 rmb_3.0 项目根目录，除非明确标为 Veil 源码路径。
- 配置字段存在，不等于运行环境一定使用该值；非空常量也不等于已经验证仍有效。
- 业务 DTO 或数据库字段存在，不等于已在仓库中发现真实个人数据。
- 未连接任何业务服务、未验证凭据、未解析二进制证书、未扫描完整 Git 历史。
- 文档只保留字段名、类别、形态和位置，不复制密码、私钥、会话 token 或个人值。

### 1.2 当前 Veil 的重要限制

本次复核当前工作区源码：

- `crates/veil/src/proxy/forward.rs:58` 仍固定使用 `builtin_ruleset()`。
- `crates/veil/src/admin/api.rs:223` 保存 `cfg.rules`，但保存链路未把配置规则切换到实际代理规则集。
- 自定义允许列表也不能仅因保存成功就视为已生效。
- 内置规则覆盖部分 PEM、API Key、Bearer、连接串、密码、身份证、手机号和 IP 形态；邮箱默认关闭。
- JSON 递归处理的是字符串值；数字值原样保留，也没有自动把所有业务字段路径传给匹配器。因此不能保证数值型账号或独立字符串 secret 被识别。

**以下“建议开启”“需要补充”均是目标策略，不表示已完成配置或已经验证生效。** 以当前源码落地时，先修复配套方案中的 P0 规则接入问题。

## 2. S0：必须脱敏的凭据与控制能力

### 2.1 数据库与中间件密码

**涉及内容：** MySQL、Redis、Elasticsearch、Nacos 等的认证口令，以及含用户名/密码的连接串。

**需要识别的形式：**

- `password`、`passwd`、`repositoryPassword` 等明确凭据字段。
- YAML 中嵌套的数据库、Redis、Nacos、Elasticsearch 配置。
- JSON 引号包裹字段、命令行 `-D…password=…`、XML 配置及日志中的赋值。
- 环境变量表达式中携带的真实默认口令。

**处理要求：** 遮盖密码实际值；用户名、纯主机地址、无口令的数据库名一般归 S2。若连接串已经整体替换，不再强求内部 IP、库名分别出现别名。

**内置覆盖：** `password` 与 `connstr` 能匹配部分形式，但短口令、JSON 键引号及嵌套上下文不能仅凭规则名称判断已覆盖。

### 2.2 云平台、第三方接口和模型调用凭据

**涉及内容：** 阿里云 AK/SK、OSS/STS/短信/SLS 凭据，飞书、企微、极光、CDS、网货、OCR、隐私通话接口及模型/工作流 API 凭据。

**代表字段与形态：**

- `accessKeyId`、`accessKeySecret`、嵌套 `access_key.id/secret`。
- `app-secret/appSecret`、`corp-secret/corpSecret`、`master-secret`、`secret-key`、`encodingAesKey`。
- `api-key/apiKey`、Coze `token`、接口上下文中的 `private-key`。
- `LTAI…`、`sk-…`、`sat_…`、日志文本中的 `APPCODE …`。

**处理要求：** AK ID 虽不是 Secret，但属于凭据对的一部分，默认一同遮盖；这不等于普通 `appId`、`merchantId` 都是秘密。SDK 的 app key 需按用途区分，不能一概按字段中有 `key` 就判定风险。

**内置覆盖：** 部分 sk 等前缀已存在；LTAI、sat、项目特有 secret 字段、APPCODE 需补充。独立随机字符串、UUID 形接口秘密不能依靠前缀规则识别。

### 2.3 签名、支付与解密密钥

**涉及内容：** JWT 签名 secret、微信支付 `apiV3Key`、支付签名私钥、汇付等接口私钥、证书解锁密码及其他承担加密/认证功能的秘密。

**代表字段与形态：**

- `jwt.secret` 或 YAML `jwt:` 下的裸 `secret:`。
- `apiV3Key`、`privateKey`、`rmbPrivateKey`、`private-key`。
- PEM 块、无 PEM 头尾的 Base64、YAML 多行 `|`/`>` 值。
- `.pfx/.p12` 等证书容器和关联密码。

**处理要求：** 完整遮盖私钥，不能只遮盖头部或第一行；单独的公钥、公开证书和证书序列号不能与私钥混为一谈。未解析容器时，对可能含私钥的附件按高风险禁止直接外发，不能只对文件路径作替换就认为附件安全。

**内置覆盖：** 部分 PEM 私钥形态存在；裸 Base64、多行配置、嵌套 secret、支付专属字段需要上下文识别。UUID 形 `private-key` 是接口凭据，不应仅因不像 PEM 而放过。

### 2.4 用户会话和浏览器登录状态

**涉及内容：** JWT、access/refresh token、实际具备会话能力的 Cookie/localStorage 值、一次性登录交换码，以及包含这些值的 Playwright 登录状态文件。

**处理要求：** 完整遮盖 token 的所有段，保留文件结构和字段名；不能仅把浏览器状态中的用户 ID 隐藏而留下认证凭据。

**内置覆盖：** Bearer 有候选覆盖；内置原始 JWT 分支匹配到第二个点为止，不能当作完整三段保护。需完整 JWT 规则及无 Bearer 测试。不是每个 Cookie 都是认证凭据，也不是整个 localStorage 都应无差别替换。

### 2.5 构建、部署与发布认证

**涉及内容：** 私有 Maven/镜像仓库口令、构建参数中的登录密码、部署命令中的 secret、直接写入部署 YAML 的凭据值。

**处理要求：** Gradle、CI、部署配置与业务 YAML 使用同一 S0 标准。`secretKeyRef`、`imagePullSecrets` 等引用项的名字本身不是凭据值；实际 `data/stringData`、文字口令或运行时解析出的值才需要 S0 处理。

## 3. S1：高优先级个人、账户与资金数据

### 3.1 身份和联系信息

- 手机号、身份证号、姓名、实际个人地址、联系人信息。
- 司机、用户、商户联系人等业务对象中的真实值。
- 包含姓名/手机号的组合标识，如某些日志 owner 标识。

手机号/身份证沿用内置基础规则，并补充上下文测试。姓名不应以“任意两三个汉字”匹配；实际地址不应以字符串长度识别。邮箱按需要启用，注意包管理、提交信息和示例邮件地址的误报。

### 3.2 银行、支付账户与客户关联信息

- 银行卡号、资金账号、收款账户、付款账户、账户持有人。
- `merAcctNo`、`serviceFundAccount` 等实际资金账户值。
- `headCustNo`、`cifNo`、`headUserId` 等客户关联标识，按业务含义确认。

真实账户属于 S1；支付私钥、签名 secret 属于 S0；商户/机构配置编号未必是个人账户，通常归 S2。不要因字段以 `No/Id` 结尾就把全部数字遮盖，也不要因不是密码就公开真实资金关系。

### 3.3 可关联个人的业务标识

- 实际 `openid/unionid`。
- 用户 ID、profile ID、JWT 中的 `sub/pid`，以及日志里的用户身份 Header 值。
- 真实业务数据中能够关联个人的订单、运单、交易关联标识。

字段定义和测试用假值可以保留；生产对象的真实值按 S1 处理。普通 trace ID 原则上保留，但若能直接换取用户数据、包含个人信息或指向可访问的私有资料，则需要升级保护。

### 3.4 真实业务记录

实际订单、运单、结算、交易金额、账户余额、合同/票据内容可能形成商业秘密或个人资金数据。不是所有“金额”都必须遮盖：用于分析计算错误的合成金额应保留；真实批量业务记录应按授权范围匿名化。

此项是数据分享策略，不表示本次已在配置文件中发现完整生产记录。

## 4. S2：按场景脱敏的企业与基础设施信息

| 内容 | 典型形式 | 建议策略 |
|---|---|---|
| 企业域名与环境地址 | 前端 API Base URL、网关地址、企业域名后缀 | 外部分享启用；排查代理、CORS、环境切换时可保留必要协议、端口和路径 |
| 主机与云实例地址 | 私有网段、RDS/Nacos/OSS 实例域名 | 采用本企业范围，不全局遮盖所有云厂商域名 |
| 数据库与配置空间 | 库名、namespace、Nacos dataId、数据库用户名 | 按上下文处理，优先保留 test/prod 等环境语义 |
| 镜像与仓库位置 | 企业 registry、namespace、镜像路径 | 外部材料可遮盖企业归属；保留版本和技术栈信息 |
| 应用/商户/机构标识 | `appId/corpId/mchId/merchantId`、证书序列号、机构编号 | 通常不是秘密；对外材料按需隐藏，不能与 appSecret/私钥同级处理 |
| 企业名称与项目代号 | 中文企业名、历史项目名、内部工作区名 | 精确词表，避免全局匹配 RMB、包名或所有业务词 |
| 私有资源对象 | OSS bucket、含业务身份的对象 key、非公开资源链接 | 按实际可访问性和内容升级；带授权签名 URL 中的授权参数按 S0 处理 |

**IP 的现状与风险分类不同：** IP 一般属于 S2，但当前内置 IP 规则默认开启，除默认允许的回环地址外，公共/私有 IP 都可能被替换。本文不是声称当前实现只脱敏内网 IP。

## 5. S3：通常保留，不应机械脱敏

- 配置字段名、层级、数据类型、空值、布尔值和普通阈值。
- 模块名、类名、接口路径、端口、公共协议名、框架/依赖版本。
- Spring 服务配置结构、路由与超时参数、日志等级、功能开关。
- 无秘密默认值的 `${ENV_NAME}` 或 `${ENV_NAME:}` 引用，环境变量名本身。
- `secretKeyRef.name/key`、`imagePullSecrets.name` 等引用结构；必要时仅按 S2 处理对象名称。
- 已确认的合成测试数据和公开示例值。
- 独立公钥、公开证书结构、普通证书文件路径；仍需检查文件或 URL 是否包含其他秘密。
- 普通请求/trace ID，以及不含真实值的 DTO、SQL schema 和类型定义。

这是保留策略，不建议把上述所有字符串都加入允许列表。允许列表仅用于具体误报值，并需逐项验证，不能放行整个字段、目录或企业网段。

## 6. 配置表示方式同样影响脱敏

| 表示方式 | 风险判断 | 处理要求 |
|---|---|---|
| 非空字面常量 | 按字段实际用途判断，凭据按 S0 | 不因位于 dev/test 就降低等级；不凭是否高熵判断 |
| 空凭据、空环境引用 | 当前文本不含秘密，不等于运行时没有秘密 | 保留结构，保护之后读到的实际运行值 |
| `${NAME:真实默认值}` | 默认值可能是凭据 | 遮盖秘密默认值，尽量保留变量名 |
| 注释中的旧凭据 | 仍可被复制或误用，可能尚未撤销 | 按实际秘密类别处理，不能跳过注释 |
| YAML 多行私钥 | S0，单行规则容易漏掉后续部分 | 完整提取和替换，不留下尾行 |
| URL query / 日志字符串 | 承载的 secret/token 与配置同级 | 区分敏感参数与普通 appid，防止值延伸或截断 |
| 嵌套 JSON/YAML | 字段路径可能才是敏感性依据 | 如 `jwt.secret`，不能只匹配任意裸 secret |
| XML / Kotlin / Gradle / 命令行 | 同类秘密使用不同语法 | 分别构建测试，不能把 YAML 通过视为全部通过 |
| 数字型账号 | 可能是 S1，不应仅扫描字符串 | 需字段上下文或文本序列化测试，不把所有数字视为秘密 |
| 二进制或编码附件 | 可能携带 S0/S1 数据 | 单独的附件策略，不宣称文本正则已经覆盖 |

## 7. 当前配置与源码的代表性证据

本节列出实际字段、典型位置和保护含义；只记录匿名化状态，不记录真实值。文件样本不是完整盘点。

### 7.1 前端配置：本轮已核实

| 位置 | 实际字段 | 观察状态 | 等级与处理 |
|---|---|---|---|
| `frontend/logistics-platform/packages/logistics-platform-web/.env.prod`；`frontend/trade-platform/packages/trade-platform-web/.env.prod` | `VITE_BASE_URL`、`VITE_BASE_PATH` | 存在非空配置，其中 Base URL 包含环境地址 | URL/企业归属为 S2，路径通常 S3；不是用户数据或认证凭据 |
| `frontend/logistics-driver-app/src/config/config-base.ts` | `baseUrl`、`baseUrl2`、`energyExternalUrl`、`devUrl`、`testUrl`、`uatUrl`、`productionUrl`、`energyTestUrl`、`energyProductionUrl`、`ossUrl`、`sensorsProdUrl`、`sensorsUrl`、`sensorsTestUrl` | 多项配置为不同环境或资源 URL | S2；需要企业信息模式时处理主机/企业归属，保留必要路径和环境关联 |
| `frontend/logistics-driver-app/src/manifest.json`；`frontend/trade-platform/packages/trade-platform-app/src/manifest.json` | `appid`、部分 `appkey_ios/appkey_android`；司机端另有 `key/aMapKey` | 同类字段存在非空配置，也有空项 | appid 通常 S2；地图/客户端 SDK key 暂按 S2 管理，核实用途与权限后决定是否升级 S0 |
| 上述 manifest 配置 | `com.apple.developer.associated-domains`、`urlCheck` | 核实存在配置项，未据此确认限制是否有效 | 域名 S2，控制项结构 S3；不能把“字段存在”当作域名限制已启用 |

前端 key 常需要随客户端分发。安全重点还包括来源域名、包名/签名、API 权限和配额限制；如果某个 key 实际可用于服务器高权限调用，应升级 S0，而不是因为位于前端就降低风险。

### 7.2 后端配置：本轮已核实

| 等级 | 实际字段 | 代表性证据 | 当前文本状态及处理 |
|---|---|---|---|
| S0 | `spring.datasource.password`、`spring.datasource.primary.password`、`secondary.password`、`spring.elasticsearch.password`、`spring.data.redis.password` | `backend/application-prod-yaml/user.yaml:5`；`backend/application-prod-yaml/platform.yaml:6–38` | 抽查见非空常量，密码值必须遮盖 |
| S0 | `spring.cloud.nacos.discovery.password` | `backend/application-prod-yaml/gateway.yaml` 等生产配置 | 已见非空密码常量；用户名、namespace 与密码分级处理 |
| S0 | `ali-sts.access_key.secret`、`oss.access_key.secret`、`teamy.oss.access_key.secret` | `backend/application-prod-yaml/user.yaml:18–32`；`backend/user-service/src/main/resources/application.yaml:30` | 已见非空常量；另有 `ali-oss/ali-sms.access_key.secret` 显式空项，不能据此判断运行环境无凭据 |
| S0 | `jwt.secret`、`wechat.apiV3Key`、`wechat.privateKey` | `backend/pay/src/main/resources/application-pay.yaml:10,28,33` | 非空常量；签名密钥、支付密钥与私钥均完整保护 |
| S0 | `zhiyun.ocr.private-key`、其他 `jwt.secret` | `backend/user-service/src/main/resources/application.yaml:15,37` | 非空常量；OCR 接口秘密不必具有 PEM 私钥形态 |
| S0 | `coze1.workflow1.token1`、`coze.workflow.token`、`huifu.rmbPrivateKey`、`parameter.password` | `backend/application-prod-yaml/buyer.yaml:4,90,454,496`；`backend/application-prod-yaml/platform.yaml:117` | 非空常量；特别注意带数字后缀的 `token1` 和裸 token，已有候选字段列表不能视为穷尽 |
| S0 字段语义，引用文本通常 S3 | `jwt.secret`、`oss.access_key.secret` | `backend/information-service/src/main/resources/application.yaml:18–33` | 属性引用，未见非空默认值；保留引用名，保护最终解析到的实际秘密 |
| S1 | `network-freight.currentUserMobile` | `backend/application-prod-yaml/logistics.yaml:58` | 非空联系方式常量；遮盖实际值，不在清单中复制号码 |
| S1 | `teamy.map.notify.phone`、`work-order.handle_phone`、`boHaiBank.serviceFundAccount` | `backend/application-prod-yaml/platform.yaml:74,85,98,107` | 非空联系方式/资金账户配置，不等于已发现交易流水；资金账户完整保护 |
| S2，具体业务用途需确认 | `parameter.merchantId`、`wechat.mchId`、`wechat.mchSerialNo` | `backend/application-prod-yaml/platform.yaml`；`backend/pay/src/main/resources/application-pay.yaml` | 商户/证书标识，不等于支付密钥；对外按需遮盖。能关联个人账户或授权能力时升级 |
| S2 | `spring.datasource.url/jdbc-url`、`username`、`spring.cloud.nacos.discovery.namespace/username`、`ali-oss.bucket`、`oss.endpoint`、`access_key.id` | `backend/application-prod-yaml/gateway.yaml:6–34`；`backend/application-prod-yaml/user.yaml:3–31` | 非空地址和标识；`access_key.id` 默认随凭据对一起隐藏，但其本身与 secret 的认证能力不同 |
| S3 | `jwt.token.validity`、`wechat.signType`、`management.endpoints.web.exposure.include`、`management.endpoint.health.show-detail` | `backend/pay/src/main/resources/application-pay.yaml:4–12,26`；`backend/logistics/driver-portal/src/main/resources/application.yaml:23,37` | 保留值和结构帮助排障。端点暴露是否合理属于配置安全检查，不是脱敏分类本身 |

本轮确认了配置中的部分非空联系信息和资金账户，因此不能笼统说“没有任何 S1 配置值”；同时，本轮没有读取实际数据库、生产请求或完整业务流水，也不能扩展为“已发现所有用户数据泄露”。

`access_key.id` 是本轮确认的实际拼写；`accessKeyId` 等其他拼写来自既有方案/类别设计，本轮没有逐一复核。上述内容未验证运行时实际来源、凭据有效性或证书内容，不能形成“当前生产仍使用仓库中这些值”的结论。

### 7.3 既有方案记录的线索与本轮未复核项

| 范围 | 既有方案/前次抽查记录 | 本轮状态与注意事项 |
|---|---|---|
| 测试登录态 | `test/logistics-driver-app/.auth/logistics-driver-user.json` 等曾记录原始 JWT 线索 | 本轮仅定位登录态路径，未读取状态内容；不能据文件名确认当前含 JWT 或仍有权限 |
| 构建配置 | `backend/build.gradle.kts` 等曾记录仓库认证和构建参数线索 | 本轮未读完整构建内容；`repositoryPassword`、`jib_password` 等作为重点复核项，不宣称已重新验证 |
| 部署 YAML | `devops/deploy/prod/` 前次记录存在 secret 引用 | 本轮定位路径，未验证 literal 或引用值；不能宣称当前部署文件含明文 secret |
| 日志/URL 文本 | 前次记录网关原始 JWT、微信 query 和 APPCODE 形式 | 本轮未逐条复核日志语句；保留为规则测试需求，不推断已捕获真实生产日志 |
| 业务身份/资金字段 | 前次记录手机号、身份证、openid、银行/支付账号及关联标识 | 本轮未读取实际业务数据；第 3 节是保护分类，不是当前仓库真实用户数据泄露清单 |
| XML、多行私钥、证书容器 | 前次记录 XML 云凭据和私钥/证书路径 | 本轮未完成专项解析；识别能力和容器是否含私钥需另行验证 |

本轮另定位到 `backend/user-service/src/main/kotlin/cn/teamy/common/domain/CommonUser.kt` 和 `backend/pay/src/main/kotlin/cn/teamy/pay/payment/dto/PaymentDto.kt` 等业务代码，但未核实全部字段定义。本文不把关键词命中或 DTO 文件存在当作个人/资金实值存在的证据。

## 8. 与已有适配方案的对应关系

| 信息等级 | 对应适配项 | 默认目标 | 主要缺口 |
|---|---|---|---|
| S0 数据库/中间件 | 内置 password、connstr + 格式补充 | 开启 | 短口令、带引号键、上下文及运行配置接入 |
| S0 云/API 凭据 | `rmb_aliyun_ak`、`rmb_coze_token`、`rmb_secret_field_line` | 开启 | 嵌套 secret、特殊字段、不同语法形式 |
| S0 会话 | `rmb_jwt_full`、`rmb_appcode`、`rmb_url_secret` | 开启 | 完整 JWT、无 Bearer、query 和会话 Cookie |
| S0 私钥/签名 | 内置 PEM + 值级/上下文扩展 | 开启 | 裸 Base64、多行私钥、jwt.secret、apiV3Key |
| S1 个人数据 | 内置 phone/idcard + 个人/资金字段扩展 | 真实数据分享时开启 | 姓名、账户、openid、数值型 ID、字段路径 |
| S2 企业信息 | `rmb_corp_domain`、`rmb_db_name` + 经确认词表 | 企业信息模式按需开启 | 企业实例范围、业务标识细分，避免误伤 |
| S3 普通结构 | 保留字段名与结构 | 保留 | 当前完整匹配替换可能连字段名一起遮盖 |

配套方案中的正则均是候选设计；本文不重复增加未经验证的长正则，也不把风险分级直接转成一组保证有效的运行配置。

## 9. 推荐的三种使用策略

以下为目标策略设计，不是当前产品已经具备的 profile 功能。

### 9.1 日常开发排障

- 始终保护 S0。
- 真实个人、资金数据按 S1 保护，不因为是开发场景就自动放行。
- S2 仅按公司要求启用必要项，尽可能保留服务名、路径、环境关系。
- 保留 S3，优先让模型看懂配置结构和错误上下文。

### 9.2 对外配置、日志与截图分享

- 保护 S0、S1，同时开启企业信息 S2。
- 文本代理无法覆盖截图中文字时，在截图制作或分享前另行处理。
- 不是只替换密码；检查 URL、登录态、注释、请求和响应报文。

### 9.3 真实生产数据分析

- 优先在本地或经批准的环境处理，只发送必要的匿名化片段。
- 保护 S0/S1，结合授权要求处理 S2。
- 使用合成 ID 保持订单、用户与账户之间的关系；不得为了排障保留可认证的原始 token。
- 金额、时间等如需保留，应评估结合其他字段是否能重新识别个人或交易。

## 10. 按重要程度的落地顺序

### 第一批：阻止凭据直接外发

1. 修复 Veil 自定义规则和允许列表的运行接入。
2. 覆盖数据库/中间件密码、支付私钥、JWT 签名 secret、云 AK/SK、第三方 secret。
3. 补全原始 JWT、登录状态、Coze token、URL query 和 APPCODE。
4. 用本地 mock 捕获实际上游请求，验证完整秘密不存在，尤其检查 JWT 签名段和多行私钥尾部。

### 第二批：保护真实个人与资金数据

1. 核实 phone/idcard 基础匹配及边界。
2. 按真实业务字段覆盖账户、openid/unionid、用户标识及姓名。
3. 区分结构化 JSON 数字、字符串以及字符串内嵌的 YAML/JSON。
4. 使用假数据验证，不把真实生产值加入测试报告。

### 第三批：企业信息模式与误报控制

1. 明确企业域名、实例域名、库名和标识范围。
2. 默认保留技术结构，精确处理必要企业名称。
3. 用负例证明普通 UUID、订单 ID、appId、公共证书、环境引用和 trace ID 没有被泛化匹配。
4. 必要时采用独立 Veil 数据目录/端口，但独立实例不是规则接入缺失的修复。

### 持续事项：仓库治理

确认 Git 中非空常量或登录态的实际用途和所有者，对已暴露凭据制定轮换计划，将秘密迁移到受控存储，并增加提交前/CI 检查。Git 历史清理、凭据轮换与证书操作需要独立授权和影响评估，不由本文自动执行。

## 11. 验收清单

- [ ] 本文涉及的 S0 类别有规则、代表性合成样本和负例。
- [ ] 私钥完整保护；原始 JWT 完整保护；不遗漏 URL 或日志中的实际秘密。
- [ ] 凭据所在 dev/test/prod 环境不影响保护要求。
- [ ] 非空常量、空引用、秘密默认值、引用对象名区分清楚。
- [ ] 个人数据实际值与 DTO 字段定义区分清楚。
- [ ] 数字型账号、嵌套字段和 YAML 多行格式有明确覆盖或明确限制。
- [ ] 控制台保存版本与实际生效版本一致，默认监听器及 route 均完成测试。
- [ ] 实际上游 mock 接收端没有秘密原值；客户端响应正确还原。
- [ ] 文档、规则包及验证报告不包含真实凭据和个人数据。

本次仅新增分类文档，没有修改 rmb_3.0、Veil 程序或运行配置，未运行端到端脱敏测试，也未执行凭据轮换。
