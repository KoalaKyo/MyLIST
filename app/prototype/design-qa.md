# Design QA — 桌面待办 HTML 原型

## 验收结论

- 日期：2026-08-20
- 视觉真源：`E:\Codex\Todo_List\design\concepts\concept-1.png`
- 实现截图（明亮）：`E:\Codex\Todo_List\design\screenshots\prototype-light-360x520.png`
- 实现截图（黑暗）：`E:\Codex\Todo_List\design\screenshots\prototype-dark-360x520.png`
- 紧凑尺寸截图：`E:\Codex\Todo_List\design\screenshots\prototype-compact-320x360.png`
- 并排对照：`E:\Codex\Todo_List\design\screenshots\qa-comparison-light-final.png`
- 对照状态：明亮模式、待办标签、4 条示例任务、未置顶。

## 对照方法

将视觉基准与最终 HTML 截图放入同一张并排对照图，统一按 520 px 高度检查。视觉真源本身就是完整的 360 × 520 小组件，因此全视图同时覆盖了标题栏、标签、列表、添加入口和状态栏；没有另设局部裁切，以免重复同一证据。

## 视觉检查

| 范围 | 结果 | 说明 |
| --- | --- | --- |
| 窗口比例与层级 | 通过 | 维持 360 × 520 的紧凑小组件比例、圆角、半透明表面和桌面阴影。 |
| 标题栏 | 通过 | 标题、置顶和设置入口与基准一致；按新增需求在中间加入主题快速切换，属于有意差异。 |
| 标签 | 通过 | 两列标签、计数徽标、蓝色短下划线和分隔线保持一致。 |
| 任务列表 | 通过 | 四条真实中文事项、完成圆圈、类型色、提醒图标和右侧时间层级一致。 |
| 类型标记 | 通过 | 第二轮将圆点标记改为基准中的浅色矩形标签。 |
| 添加入口 | 通过 | 第二轮移除过重的实色卡片，恢复为基准中的轻量底部操作。 |
| 图标 | 通过 | 全部使用 Microsoft Fluent System Icons 官方 SVG 资源，尺寸和线条统一。 |
| 明暗主题 | 通过 | 明亮模式保持基准的 Acrylic 观感；黑暗模式使用同一布局和状态色语义。 |
| 320 × 360 | 通过 | 标题栏、标签、列表、添加入口和底部状态均可用；列表内部滚动，无水平溢出。 |

## 缺陷与修复记录

1. P1 — 首轮 360 px 验证时右侧内容被裁切。
   - 原因：舞台内边距与小组件宽度计算叠加，且浏览器在 Windows DPI 下存在视口换算。
   - 修复：缩小窄屏舞台边距、显式靠左放置窄屏小组件，并为任务网格指定固定列。
   - 结果：最终截图中标题栏、时间文本、设置图标和圆角边界完整可见。

2. P2 — 首轮任务类型使用圆点，与基准的浅色标签不一致。
   - 修复：改为按类型颜色生成的浅色矩形标签。

3. P2 — 首轮添加入口使用较重的填充卡片。
   - 修复：默认改为透明轻量按钮，仅在悬停时显示柔和背景。

4. P2 — 首轮标签激活下划线过宽。
   - 修复：改为居中的 74 px 短下划线，匹配基准视觉重心。

所有 P0 / P1 / P2 差异均已处理；剩余差异仅为用户明确要求增加的主题切换入口。

## 交互回归

- 新增：填写标题、备注、类型、提醒和覆盖颜色后保存，待办数量从 4 变为 5。
- 完成：事项自动移入已完成，已完成数量从 3 变为 4。
- 恢复：事项回到待办并自动切回待办标签。
- 删除：显示确认弹窗，确认后待办数量恢复为 4。
- 主题：标题栏可在明亮/黑暗之间切换；设置中可选跟随系统/明亮/黑暗。
- 类型：成功添加“学习”类型，类型编辑控件可见。
- 导出：加密密码少于 10 个字符时阻止导出并显示错误；有效密码后关闭弹窗。
- 导入：完成选择文件、预检、确认合并和结果状态；预检显示新增 3、更新 2、保留 8、删除 1、跳过 6。
- 控制台：最终回归无 warning / error。
- 构建：`npm run build` 通过。
- 模板测试：`npm run test:sites`，4/4 通过。

## 2026-08-20 用户反馈迭代

### 新证据

- 用户提供的设置页问题截图：`C:\Users\KYO\AppData\Local\Temp\codex-clipboard-0171fca9-da05-4905-81d0-dc646e6e5bb7.png`
- 修改后主界面：`E:\Codex\Todo_List\design\screenshots\prototype-main-feedback-final.png`
- 修改后设置页：`E:\Codex\Todo_List\design\screenshots\prototype-settings-feedback-final.png`
- 更新后的主界面对照：`E:\Codex\Todo_List\design\screenshots\qa-comparison-feedback-final.png`

### 修复与验证

1. P1 — 任务时间/提醒与类型标签不在同一基线上。
   - 将任务行明确拆成标题行与元信息行，时间固定在第二行。
   - 浏览器测量前三条任务的类型中心线与时间中心线差值均为 `0 px`。

2. P1 — 设置页缺少符合 Windows 逻辑的返回操作。
   - 移除设置标题栏关闭图标，新增底部“取消 / 保存设置”。
   - 验证取消不应用黑暗主题，保存后应用黑暗主题，再保存明亮主题可恢复。

3. P1 — 输入框和按钮高度不一致，视觉节奏松散。
   - 统一表单输入、主要按钮、次要按钮为 `32 px`；浏览器实测新类型输入框、添加、取消、保存按钮均为 `32 px`。

4. P2 — 字号层级零散。
   - 建立标题 `18 px`、区块 `14 px`、正文 `13 px`、辅助 `11 px`、说明 `10 px` 五级规范；设置标题保留 `17 px` 的 Windows 面板层级。

5. P2 — 颜色控件使用“圆角方框包长方色块”，造型不协调且色彩过于生硬。
   - 改为无外框容器的圆形色样；更新为雾灰、湖蓝、松绿、琥珀、珊瑚红、鸢尾紫。

6. P2 — 主题入口占用主标题栏。
   - 移除标题栏主题按钮，仅在设置的常规页保留跟随系统/明亮/黑暗。

7. P2 — 缺少 Windows 窗口关闭入口。
   - 新增关闭按钮，并按追加反馈与置顶、设置统一为 `34 × 34 px`、`22 px` Fluent 图标。
   - 点击后模拟最小化到托盘，并显示状态提示。

### 2026-08-21 窗口缩放柄迭代

1. P1 — 右下角拖拽柄的点阵形态不符合 Windows 小组件的指定规范。
   - 修复：改为六个圆点构成的 1–2–3 等腰直角三角形；首行 1 点、第二行 2 点、第三行 3 点，直角固定在右下角。
   - 交互：点阵改为有可读名称的按钮，按住后可同时调整窗口宽度和高度；最小限制为 320 × 360 DIP，保留键盘焦点样式。
2. 回归：`npm run build` 通过；`npm run test:sites` 4/4 通过。
3. P2 — 点阵到窗口右边和底边的留白不一致。
   - 修复：将 26 px 透明拖拽热区绝对锚定在右下角；六点可视矩阵的右、下内边距均设为 8 px。
4. P2 — 底栏“桌面模式”与图钉状态重复，占用有限的小组件空间。
   - 修复：移除桌面模式文字与图标；将设置入口从标题栏迁到左下角，右上角仅保留置顶与关闭。
5. P1 — 左下角设置入口引用了不存在的 20 px 图标资源，导致图标不可见。
   - 修复：改用现有的 `settings_24_regular.svg`，以 20 px 视觉尺寸渲染。
6. P1 — 任务行占用两行，分隔线和下划线让小组件显得零散。
   - 修复：每条任务压缩为“完成勾选 / 类型 / 标题 / 时间”的单行顺序；移除主界面横向分隔线，顶部和底部改用轻微灰底区分；待办与已完成改为药丸式切换。
7. P2 — 时间颜色与待办/已完成切换的容器层级不足清晰。
   - 修复：正常时间和提醒改为浅灰，仅逾期保留危险色；切换控件改为浅灰大圆角容器，选中项使用内嵌小圆角矩形。
8. P2 — 底部添加按钮占用空间且与任务列表起点脱节。
   - 修复：删除文字添加按钮；在待办/已完成切换左侧新增紧凑加号入口，其中心与任务行完成圆圈的左侧基线对齐。
9. P1 — 分段切换容器缺少弹性布局声明，导致待办/已完成显示异常；加号入口也缺少圆形背景。
   - 修复：恢复切换容器的 `display: flex`；加号改为圆形、浅强调色背景，保持与任务完成圆圈同一中心线。
10. P2 — 底部设置图标与任务左侧圆形控件未对齐，底栏高度也不足以容纳圆形按钮。
   - 修复：设置改为 28 px 圆形浅强调色按钮并对齐左侧基线；底部浅灰区域增至 42 px（紧凑高度为 38 px）。
11. P2 — 设置按钮默认蓝色会与主要新增操作竞争视觉焦点。
   - 修复：默认改为浅灰圆形与浅灰图标；仅悬停时切换为蓝色强调状态。
12. P1 — 加号入口位置理解错误，仍位于分段控件左侧。
   - 修复：移除顶部左侧加号，将新增入口移动到任务区与底部灰色区域交界的窗口底部中央；使用 48 px 浮动圆形按钮。
13. P2 — 新增按钮使用描边，与底部灰色区域的实心控件语言不一致。
   - 修复：改为实心浅灰圆形按钮和灰色加号；悬停时切换为蓝色实心反馈，不再显示描边。
14. P2 — 实心浅灰按钮对比度不足，加号不够清晰。
   - 修复：按钮提升为中灰实心背景，加号改为白色，悬停仍切换蓝色。
15. P1 — 深色主题切换后的局部控件仍可能沿用浅色原生控件配色。
   - 修复：为窗口增加显式 `theme-dark` 标记和 `color-scheme: dark`，让输入框、弹窗和系统控件统一跟随深色变量。
16. P2 — 时间提示包含“剩余”和具体逾期时长，造成任务行信息冗余。
   - 修复：剩余时间改为仅显示数值与单位，逾期统一显示 `已逾期`；新增任务根据截止时间计算紧凑时长。
17. P1 — 设置页导航与首页分段控件不一致，分类名称过长，外观选项占用过多空间。
   - 修复：设置页导航改为首页同款浅灰外层 + 内嵌选中项；“类型与颜色”改为“分类”；外观改为下拉菜单。
18. P2 — 设置页仍有横向分隔线，顶部和底部缺少层次区分。
   - 修复：移除设置标题、内容分组和底部操作区的横线；顶部与底部改用浅灰背景区分。
19. P1 — 设置页保留“本地设置”眉题、标题区高度与首页不一致，底部灰色区域不明显。
   - 修复：移除眉题；标题区改为 60 px；导航恢复 48 px 行高；底部操作区改为清晰的浅灰 42 px footer。
20. P1 — 后置 `.settings-actions` 规则将底部背景覆盖回 `var(--panel-solid)`，导致灰色区域不可见。
   - 修复：统一后置规则为浅灰背景并保留 42 px footer 内边距。
21. P2 — 设置底部灰色区域高度不足，取消/保存按钮上下留白过小。
   - 修复：设置 footer 增至 54 px，按钮区上下内边距调整为 11 px；紧凑高度保留 50 px。
22. P1 — 操作色彩仍以低饱和蓝色为主，确认、选中和悬停状态缺少统一的强行动色；按钮字号也存在散落值。
   - 修复：新增 action/action-hover/action-soft/action-pressed token；确认、添加、选中、开关、焦点和悬停统一黑色行动色，危险操作保留红色；按钮文字统一使用 12 px token。
23. P1 — 分段控件选中态使用黑底白字，在主界面信息层级中视觉过重。
   - 修复：待办/已完成及设置页分段控件的选中项改为白底黑字；黑色行动色仅保留给确认、开关和悬停反馈。
24. P1 — 底部新增入口默认灰色，主要行动不够突出。
   - 修复：浮动添加按钮改为黑色实心白色加号，悬停使用 action-hover。
25. P1 — 任务完成圆圈沿用类型色，未完成状态视觉不统一。
   - 修复：未完成统一黑色轮廓圆圈；已完成统一黑色实心圆圈并使用白色勾选，保持可读对比度。

28. P1 — 分段标签顺序与数字徽标样式需要进一步统一。
   - 修复：数字前置，文字后置；选中数字徽标使用黑底白字。
29. P1 — 按钮交互状态需要统一，避免不同控件反馈不一致。
   - 修复：统一补齐正常、悬停、按下和禁用状态；主要操作使用黑色行动色，危险操作保留红色语义；拖拽点阵、颜色选择、导入区域和分类选项也补充悬停反馈。
30. P1 — 任务行操作按钮常驻且标题区域出现独立悬停底色，影响整行信息层级。
   - 修复：移除标题按钮独立悬停背景；待办行悬停仅显示圆形更多菜单，编辑与删除收进菜单；已完成行悬停仅显示圆形删除按钮；右侧操作出现时隐藏时间和提醒。
31. P1 — 更多菜单可能被后续任务行的提醒文字覆盖，且菜单关闭后存在悬停误触风险。
   - 修复：打开菜单的任务行提升层级，弹层使用独立高层级；点击菜单区域外关闭；菜单仅由点击按钮切换，悬停不会打开。
32. P2 — 菜单打开后按钮缺少持续的按下反馈。
   - 修复：菜单按钮在打开期间保持黑色激活态，并同步 `aria-pressed`；菜单关闭后恢复普通状态。
33. P2 — 任务操作菜单与设置页下拉控件的圆角和选项反馈不一致。
   - 修复：统一菜单外框为 8 px 圆角、选项高度与间距；悬停/焦点使用黑底白字，原生下拉选中项同步黑底白字。
34. P2 — 设置页未选中标签悬停时改变底色，破坏分段控件层级。
   - 修复：设置页未选中标签悬停仅改变文字颜色，保持原有底色不变。
35. P1 — 设置状态下右下角无法继续使用窗口缩放热区。
   - 修复：在设置面板内复用同一拖拽逻辑，保留右下角透明 26 px 热区并隐藏六点图标。
36. P2 — 主界面与设置页分段控件的未选中悬停规则未完全同步。
   - 修复：所有 `.main-tabs` 同类控件统一为未选中悬停只变文字颜色，选中态保持白底黑字。
37. P2 — 未选中的任务数量徽标缺少与选中徽标一致的圆形容器。
   - 修复：未选中数量改为 20 px 白色圆形底，数字居中；选中数量继续使用黑底白字圆形底。
38. P2 — 未选中数量徽标的纯白底与半透明组件背景对比过强。
   - 修复：改用 `var(--surface-hover)` 浅灰底，比外层背景略深；选中态仍保持白底标签与黑底数量徽标。
39. P1 — 设置页透明缩放热区可能覆盖底部保存/取消按钮的点击区域。
   - 修复：设置 footer 提升到缩放热区之上，仅按钮接收指针事件；footer 空白区域继续透传到右下角缩放热区。
40. P2 — 浅灰数量徽标与外层背景的明度差仍然不足。
   - 修复：在原 `surface-hover` 基础上混入 10% 黑色，进一步加深未选中圆形底。
41. P2 — 数量徽标加深后与背景对比过强。
   - 修复：将黑色混入比例由 10% 调整为 5%，恢复更轻的浅灰层次。
42. P1 — 原生下拉菜单无法稳定统一圆角、选中和悬停样式。
   - 修复：设置、任务类型和提醒选择统一替换为自定义下拉控件；面板与选项使用 8 px 圆角，选中黑底白字，未选中悬停浅灰底。
43. P2 — 更多图标三个圆点的水平间距不均匀。
   - 修复：重新计算三个圆点中心，统一为 5.5 px 间距并保持同一基线。
44. P1 — 默认色板数量和展示方式不符合新的低饱和自然配色要求。
   - 修复：改为 18 个覆盖全色相的低饱和自然色，设置页以 3 行 6 列色块展示；移除颜色名称和二次编辑入口，任务颜色选择复用同一色板。
45. P1 — 分类页以颜色编辑为主，缺少以分类列表为中心的新增/编辑/删除流程。
   - 修复：改为分类列表主视图；底部新增分类按钮自动生成“新分类”并分配未使用颜色；编辑状态显示名称输入框和颜色选择浮窗，右侧按钮切换为保存；普通状态提供编辑和删除操作。
46. P1 — 色板颜色差异不足且未按色相/明度矩阵组织；“未分类”不符合当前分类模型。
   - 修复：色板调整为 21 色、3 行 7 列，列顺序为红橙黄绿青蓝紫，行顺序为偏亮/中等/偏暗，统一中等饱和度；移除未分类类型并将新任务默认分类改为工作。
47. P1 — 三行色板明度层次不足，分类列表色点存在视觉淡化感。
   - 修复：扩大亮/中/暗三行的明度跨度；分类列表色点改为 16 px 原色实心圆，不使用内阴影和透明度。
48. P1 — 设置修改依赖底部“取消/保存设置”，不符合即时生效要求。
   - 修复：移除设置底部操作按钮；主题、启动选项、分类名称和颜色修改均在变更后立即写入当前原型状态。
49. P2 — 分类内容区标题“分类”与顶部导航重复。
   - 修复：删除内容区重复标题，仅保留顶部导航标签和分类列表。
50. P2 — 删除未分类后，历史任务或导入任务可能引用不存在的分类 ID。
   - 修复：任务行回退显示当前首个分类；打开编辑时将未知分类规范化为工作分类，避免界面再次出现“未分类”。

51. P1 — 分类列表色点所在的禁用按钮继承全局浅灰禁用背景，导致实际颜色像被浅色蒙版覆盖。
   - 修复：禁用分类色点按钮改为透明背景、保持不透明度 1，色点直接使用分类原色显示。
52. P2 — 数据页“完全本地”说明模块与当前操作信息层级重复。
   - 修复：移除该说明模块，仅保留导出待办文件和导入并自动合并两个操作入口。
53. P1 — 三档色板的亮度层次和选中态外圈尺寸不够统一，分类列表色点缺少同款浅灰圆环。
   - 修复：保持暗色行不变，提高中间行和顶部行亮度；选中颜色改为与未选项同尺寸的细边框；分类列表色点增加白底浅灰外圈并保留原色内芯。
54. P2 — 设置页缺少明确的返回主界面入口。
   - 修复：标题左侧增加与右上角窗口图标一致的 34 px 圆角矩形返回按钮，使用 22 px 旋转后的 Fluent chevron 图标；按钮左边缘与内容区对齐，设置标题在顶部居中显示。
55. P2 — 设置页标题栏背景与主界面标题栏的透明层级不一致。
   - 修复：设置页标题栏复用主界面的 `--section-surface` 背景规则，确保两种主题下视觉一致。
56. P1 — 主界面顶部、标签栏、底部栏及设置页标题栏使用不同浅灰层级；图钉和返回按钮悬停背景也未与数量圆形统一。
   - 修复：新增 `--section-surface` 统一四处区域背景；新增 `--count-surface` 统一未选中数量圆形、图钉悬停和返回按钮悬停背景。
57. P1 — 设置页“添加分类”按钮仍使用独立的浅灰背景，和标题栏、导航容器不一致。
   - 修复：添加分类按钮背景改为 `--section-surface`，悬停时仍使用黑色行动色反馈。
58. P2 — 设置页“系统设置”悬停按钮仍使用独立的灰色，且 Windows 启动开关关闭态偏深。
   - 修复：系统设置按钮常规态与添加分类一致使用 `--section-surface`，悬停改为黑色实心背景和白色文字；关闭态开关使用进一步提亮的 `--toggle-off` 颜色，并在明暗主题中保持一致规则。
59. P1 — 逾期示例任务预置了红色任务覆盖色，视觉上误认为逾期状态会覆盖分类颜色。
   - 修复：逾期示例恢复为工作分类的蓝色；逾期红色仅用于时间/提醒状态，任务自定义颜色仍保持用户选择。
60. P2 — 任务行悬停和更多菜单使用独立背景，未沿用已建立的页面色彩 token。
   - 修复：任务行 hover 使用顶部区域同款 `--section-surface`；更多菜单使用未选中“已完成”数量圆形同款 `--count-surface`。
61. P1 — 同一分类的任务颜色直接读取任务记录，导致设置中的分类标准色与列表显示不一致。
   - 修复：任务增加分类颜色继承语义；未明确覆盖颜色的任务始终读取当前分类标准色，只有单任务选择覆盖色时才读取任务颜色。
62. P2 — 主界面与设置页标签栏的上下留白未由同一套间距规则控制。
   - 修复：两处标签栏统一采用距上方 8 px、距下方内容 8 px 的布局；标签容器行高调整为 44 px，任务列表和设置内容首项同步使用 8 px 顶部间距。
63. P1 — 添加事项仍显示“新任务”眉题、颜色选择区和独立的编辑头尾背景。
   - 修复：移除“新任务”眉题；添加事项颜色完全由类型标准色决定；编辑头部和底部改用主界面 `--section-surface`，底部移除分割线。
64. P2 — 自定义截止时间选择器缺少明确的默认小时和分钟。
   - 修复：无值或清除后的截止时间选择器默认小时为 18、分钟为 00。
65. P1 — 截止时间浮窗受编辑面板滚动容器裁切，小时/分钟只能显示单个值，缺少滚轮交互。
   - 修复：日期浮窗通过 portal 提升到窗口外层显示，外框和日期/时间面板统一圆角；小时和分钟显示上下多值，中心值黑色选中，支持滚轮和上下按钮；日历字号放大，时间标签缩小并统一浮窗字体层级。
66. P2 — 外置日期浮窗缺少自身主题背景变量，导致面板背景透明；字体层级又偏大。
   - 修复：为浮窗补充明亮/暗黑主题变量和实心背景，所有日期/时间文字统一为小字号，保留小时/分钟的紧凑标签。
67. P2 — 小时/分钟列宽大于日历日期选中方块，时间面板显得松散。
   - 修复：小时和分钟列统一收窄为 26 px，时间面板改为紧凑双列布局，与日历日期方块保持同一视觉尺度。
68. P2 — 时间列标签文案偏长，影响紧凑浮窗的识别效率。
   - 修复：时间列标签统一简化为“时”“分”，并同步更新辅助标签和按钮名称。
69. P1 — 时间列各元素未与日历的星期行、日期行和底部操作行对齐。
   - 修复：时间列改为固定基线：标签对齐星期行、上箭头对齐首行日期、六个时间值对应六行日期、下箭头对齐底部操作行。

### 回归结果

- `npm run build` 通过。
- 设置取消/保存、主题应用、关闭窗口模拟、任务行对齐均通过浏览器验证。
- 修改后界面无水平溢出；设置内容在紧凑高度内可滚动，底部保存操作始终固定可见。

70. P1 - All dropdown menus, task action menus, and category color popovers are now rendered through `FloatingPortal` at the document body layer. They stay outside parent scrolling and clipping contexts, auto-flip near viewport edges, and use the same compact typography and theme tokens. Light-mode menu surfaces are white.

71. P1 - The time selector column now follows the date panel's shared rhythm: time labels align to the month heading, the upper arrows align to the weekday row, six time values align to the six calendar rows, and lower arrows align to the clear/today footer row.

72. P2 - Standard dropdown and task menus inherited the compact 11px portal fallback instead of the regular interface text size.
   - Fix: add a shared `--menu-font: 13px` token to the generic portal layer. Date/time picker typography remains unchanged.

73. P1 - Home, add-item, and settings title bars had different heights and title alignment rules.
   - Fix: all three now use the same centered 60px title-bar rhythm (48px in the compact viewport), with action icons positioned independently at the edges. Add mode footer action now reads “确认添加”; edit mode remains “保存任务”.

74. P1 - Toast notices overlapped the footer add button and used long, inconsistent copy.
   - Fix: move the toast into the title-bar zone with a 34px centered overlay, keep a 160px minimum width, and shorten notices to compact four-character-or-longer messages. The main pin control now sits at the upper-left; close remains at the upper-right.

75. P2 - The title-bar toast was too wide and overlapped the pin/close controls.
   - Fix: set 56px left/right insets on the 34px title-bar buttons, leaving an 8px gap on both sides while preserving the minimum readable toast width.

76. P2 - Topmost state added an extra outline around the widget.
   - Fix: remove the topmost outline and keep the base widget shadow; the pin icon remains the state indicator.

77. P2 - Internal scrollbars were too visually prominent and retained native arrow buttons.
   - Fix: use a lighter 28% faint-color thumb, transparent track, compact rounded thumb, and explicitly hide WebKit scrollbar buttons across task, editor, and settings scroll regions.

78. P1 - The initial category set only included work and life, leaving the default classification model incomplete.
   - Fix: initialize seven categories in the requested order—work, personal, team, life, finance, study, travel—and assign each a distinct color from the 21-color palette. New categories continue to select the first unused palette color automatically.

79. P1 - Task rows duplicated category identity as text chips and used a generic black completion circle in the first column.
   - Fix: replace the first-column control with the same 20px gray-ring/12px color-dot treatment used in settings. Hide the category text chip; on row hover/focus the ring becomes the black completion circle, and completed rows use the same reversible interaction with their checkmark revealed on hover.

80. P1 - Task color circles still used a layered gray/inset treatment and the hover completion state was a solid black fill.
   - Fix: enlarge the task color dot to 16px, keep one simple 1px gray outer ring with no shadow, and change hover/focus completion feedback to a black outline circle. Pressed state keeps the outline with a subtle soft fill.

81. P2 - The task row menu button hover state used the black action fill, unlike the title-bar pin hover state.
   - Fix: menu-button hover/focus now uses the shared darker light-gray `--count-surface` background with a black icon; the open/pressed state remains black.

82. P1 - The menu-button hover override had lower selector specificity than the generic tiny icon-button hover rule, so it remained black.
   - Fix: scope the rule to `.icon-button.tiny.row-menu-button:not(.active)` with higher specificity. Closed menus now use the pin-like light-gray/black hover; open menus retain the black pressed state.

83. P2 - Close/delete hover icons used a low-saturation danger red.
   - Fix: raise the shared light-theme danger token to `#c62835` and the dark-theme token to `#ff737c`; hover backgrounds remain the shared light-gray surface.

84. P2 - The editor header repeated an edit eyebrow above the title, and the primary footer copy differed between add/edit modes.
   - Fix: remove the redundant eyebrow, keep the main title centered in the shared title bar, and use “保存事项” for the primary footer action in both modes.

85. P2 - The settings back icon aligned its button box with the content, but the chevron's visible edge sat inward.
   - Fix: shift the 34px back-button container from 18px to 12px so the icon glyph itself aligns with the settings content left edge while the title remains centered.

86. P1 - The shared horizontal alignment pass initially used a 16px baseline, but the compact widget needed a tighter rhythm.
   - Fix: revise the common baseline to 8px for title-bar controls, tabs, task rows, editor content, settings content, and footer action areas across all views.

87. P2 - The settings general/data content had no enclosing module border, so the shared 8px content padding looked narrower than bordered sections.
   - Fix: keep the global 8px rhythm for tabs and bordered panels, but use 16px left/right padding inside the unbordered settings content area.

88. P1 - The settings category page mixed `action-soft`, `section-surface`, and rectangular action-button hover treatments.
   - Fix: category row/edit surfaces now use the standard `section-surface`; edit/delete controls are 28px circular buttons with the main interface's `count-surface` hover and black/red icon semantics.

89. P1 - Completed task rows still showed the category ring as the base control, leaving the completion state visually ambiguous.
   - Fix: completed rows now always use a black filled 20px circle with a white checkmark; the category ring remains the base indicator only for open tasks.

90. P1 - Completed-row deletion opened an additional confirmation dialog instead of using an inline two-step action.
   - Fix: first click expands the fixed-right delete control leftward into a red pill with a white icon/“删除” label over 300ms; pointer input is ignored during the animation. The second click deletes immediately without a dialog or toast, while menu-based deletion keeps its existing confirmation flow.

91. P1 - Add/edit sheets still used a top-right close action, and task rows opened edit mode directly with no read-only detail view.
   - Fix: add/edit/view sheets now share a left-aligned return button and centered title bar with no top-right close control. Clicking any task opens a read-only “查看事项” sheet with the same fields locked, plus a white “编辑” action and black “返回” action; the edit action restores the existing editable flow.

92. P1 - Completed rows always displayed a black completion control, so their category color was hidden before hover.
   - Fix: completed rows now show the category color ring by default and transition to a filled black check button on row hover/focus; the completed title remains clickable and opens the read-only detail sheet.

93. P2 - The inline completed-row delete confirmation pill was wider than necessary and its hover rule lost to the generic danger-button selector.
   - Fix: shorten the pill to 68px with balanced icon/text spacing and add explicit standard, hover, and pressed danger-red tokens with white icon/text. Leaving the row resets the unconfirmed state so the next appearance is the normal circular delete button.

94. P2 - Read-only detail fields could show a browser focus outline and a white panel fill after being clicked.
   - Fix: view-mode inputs/textareas now suppress focus rings and use the shared standard light-gray surface, matching the read-only type, deadline, and reminder controls.

95. P1 - Completion controls moved tasks immediately with no confirmation animation, unlike the inline delete action.
   - Fix: clicking a todo color ring now expands rightward into a 72px hollow capsule labeled “已完成”; clicking a completed black check expands into an 84px filled capsule labeled “移入待办”. Both use a 300ms lockout, hide overlapping row content while open, reset when the pointer leaves, and commit only on the second click without an extra icon.

96. P2 - The completion confirmation capsule was shorter than the delete confirmation control.
   - Fix: both completion capsules now use the same 28px height as the delete confirmation button.

97. P1 - Moving a task between todo and completed lists could preserve the previous row component's expanded confirmation state.
   - Fix: reset completion confirmation and animation state whenever the task status changes, so each list always renders its default category ring or completed check state after the move.

98. P1 - The completion capsule hid the title, time, and right-side actions while open, preventing users from seeing the row context.
   - Fix: completion-open rows now expand their first grid column to the capsule width, keep the title/time/actions visible, and let the title column shrink naturally while preserving ellipsis.

99. P2 - Long task titles had no external full-text affordance when ellipsized.
   - Fix: truncated task titles now open a viewport-level Tooltip with the complete title on hover/focus; short titles do not create a Tooltip.

100. P2 - The home title still used the temporary product name.
   - Fix: the visible home title, accessible widget label, and document title now use “MyLIST”.

101. P1 - The completion capsule width changed immediately at the grid level, so the title did not visibly move with the 300ms button animation.
   - Fix: animate the task row's first grid track from the circle width to the capsule width alongside the button, pushing the title smoothly from left to right.

102. P1 - The completion-open override made the row's time reminder visible even while the row was hovered.
   - Fix: completion-open rows continue to hide the time/reminder area on hover while keeping the title and right-side operation area available.

103. P1 - The completion capsule could appear at the same time as the completed-row delete button, and outside clicks did not consistently collapse it.
   - Fix: the row keeps both controls visible, but opening one collapses the other; any pointer action outside the expanded completion button collapses it while a second click on the capsule confirms the state change.

104. P2 - Completion and delete controls could show a generic black focus ring around the pill.
   - Fix: completion and inline delete pills suppress the surrounding focus outline while retaining their own hover/pressed visual states.

105. P1 - The completion animation was interpreted as a control-size change, which would alter the established circle dimensions.
   - Fix: preserve the existing completion circle dimensions and keep the 28px height only on the expanded capsule; the animation interpolates width from the current circle to the capsule without resizing the base control.

106. P1 - The completion width transition was overridden by the generic button transition, and the expanded state lost its leading category circle.
   - Fix: give the completion control a higher-specificity 300ms width/height/transform transition, keep the base circle at 20px, scale it to the established 28px hover size, and retain the leading circle with a 10px gap before the confirmation label.

107. P2 - The expanded completion control still included the leading circle/check icon, while the requested expanded state was text-only.
   - Fix: expanded completion/recovery controls are now pure-text capsules with symmetric 8px horizontal padding, matching the delete confirmation button's text-to-edge spacing; the original 20px circle remains only in the collapsed state.

108. P1 - The task title did not visibly move during completion capsule open/close because the generic button transition overrode the title transition.
   - Fix: add a higher-specificity task-main transition so the title column shifts right and returns to its original position over 300ms in sync with the capsule width; overflow remains ellipsized and long-title Tooltip behavior is preserved.

109. P1 - A completed task could keep the black check icon after the pointer left because `:focus-within` was treated as the completed-row hover state.
   - Fix: completed-row category/check swapping now follows actual pointer hover only; after leaving the row, the type-color ring is restored even if the button retains keyboard focus.

110. P1 - Leaving an expanded completion capsule swapped its icon content immediately instead of waiting for the collapse motion to finish.
   - Fix: pointer exit and outside clicks now schedule a 300ms completion collapse; the text capsule and shifted title remain during the transition, and the category-color ring/check state returns only after the animation completes.

111. P2 - The long-title Tooltip check measured the outer task button, whose scroll width stays constrained by the grid, so an actually ellipsized title did not open its full-text Tooltip.
   - Fix: compare the inner `.task-title` scroll width with its client width, matching the element that owns the ellipsis; the test data now includes a deliberately long todo title.

112. P1 - Todo rows could retain the black hollow completion circle after pointer exit because the shared completion styles treated `:focus-within` as hover.
   - Fix: completion color/check swapping now responds only to actual row/button hover for both todo and completed rows; keyboard focus no longer leaves a stale hover icon after the pointer exits.

113. P2 - The active white list/settings tab inherited the global press brightness filter and became gray while pressed.
   - Fix: active tab press keeps the same white surface and explicitly disables the global brightness filter, while text and focus treatment remain unchanged.

114. P1 - Dark-mode data cards used the black action color for their leading icons, making export/import icons low-contrast on the navy surface; contextual date controls also lacked explicit dark action press tokens.
   - Fix: use the muted readable icon color for data actions, add explicit light/dark action-soft and action-pressed tokens to the date popover, theme destructive buttons through danger tokens, and reduce the dark acrylic highlight overlay.

115. P1 - Dark mode still used light translucent acrylic surfaces and a pale toast, which weakened the darker visual hierarchy and conflicted with the blue confirmation color.
   - Fix: use opaque light/dark surfaces throughout the widget and portals, deepen the dark stage/widget palette, remove the acrylic highlight overlay, use blue action/hover/pressed tokens in dark mode, and make toast feedback follow the same action color.

116. P2 - Small row-action, footer, privacy-note, and success-dialog surfaces still mixed their color with transparent backgrounds, producing inconsistent depth against the new opaque theme layers.
   - Fix: use opaque theme surfaces or opaque semantic blends for these supporting controls and notices while preserving the dialog backdrop as the intentional dimming layer.

117. P1 - The dark theme still presented white active tabs and overly bright blue/white contrast, while the stage retained a pale radial highlight.
   - Fix: introduce a lower-contrast cool blue token set, use dark elevated surfaces for active tabs, mute primary/secondary text and category swatches, replace the dark stage highlight with cool blue-black gradients, and suppress dark-mode brightness filters.

118. P1 - Dark-mode confirmation, hover, pressed, focus, danger, and portal states were not guaranteed to use the same semantic tokens as the widget.
   - Fix: align the widget, date popover, and portal token sets; use blue confirmation states in dark mode, subdued danger states, blue focus outlines, and verify that no dark-mode button computes to a white background.

119. P1 - Date and contextual portal text inherited the document's dark-on-light root color, and the toggle knob remained a light-mode white surface in dark mode.
   - Fix: explicitly inherit portal text from the theme tokens, add matching light/dark focus tokens, and use a cool light-blue toggle knob on dark surfaces. Rechecked main, settings, category, data, add-item, and date-picker states with no visible white button surfaces.

120. P1 - The category palette still exposed 21 colors and default task categories were not mapped to the reference image's middle row.
   - Fix: replace the palette with the extracted 3×8 values from the supplied reference, keep the middle row as the category source of truth, map 工作/个人/团队/生活/财务/学习/出行 to the corresponding middle-row swatches, and update the picker grid to eight columns.

121. P1 - Dark surfaces were too gray and inconsistent between the main list and settings sheet; selected tabs and shared icon-button hover surfaces also had excess contrast.
   - Fix: unify all dark page bodies on `#070b12`, keep title/footer/elevated surfaces one restrained step brighter, use a saturated blue confirmation scale, remove selected-tab outlines, set dark toggle knobs to the base background, and raise the shared hover surface to `#1c2b43`.

122. P1 - The reference palette extraction needed to preserve the supplied image's exact 3×8 ordering and values rather than approximate colors.
   - Fix: use the sampled reference values in row-major order: `#D6E5FF #D6F1FF #D3F3E2 #FFDCDB #FFECDB #FFF5CC #FBDBFF #FFDBEA`, `#ADCBFF #ADE4FF #ACE2C5 #FFB5B3 #FFCEA3 #FFEA99 #E7B4FF #FFB3DC`, and `#2972F4 #00A3F5 #45B076 #DE3C36 #F88825 #F5C400 #9A38D7 #DD4097`; the picker now renders all 24 swatches without clipping.

123. P1 - The reference palette's middle row needed stronger saturation and the dark row needed lower brightness without changing the light row.
   - Fix: keep the first eight swatches unchanged, update the middle row to saturated `#8CB9FF #7DD8FF #77D6AC #FF9390 #FFB774 #FFE36A #D294F4 #FF93C6`, darken the third row to `#1A4FBC #007DBB #2E7D52 #A9282A #B95612 #B58E00 #6E219E #A72E6E`, and remap all default category/task colors to the new middle-row values.

124. P2 - Settings category icons used a smaller center swatch with a thick gray ring and inner-shadow treatment instead of matching the color-picker swatches.
   - Fix: enlarge the center swatch to 16px, use a single 1px theme border on the 20px ring, remove the inner-shadow/extra outline, and apply the same geometry to read-only task type controls.

125. P2 - Danger red on close, overdue, delete, and related states still looked muted against the light and dark surfaces.
   - Fix: increase danger saturation consistently across widget, date popover, portal, hover, and pressed tokens while leaving the category palette red swatches unchanged.

126. P2 - The dark preview canvas outside the widget still used a blue-black gradient instead of the requested pure-black surround.
   - Fix: set the dark prototype stage background to opaque `#000` while keeping the widget's internal dark surfaces unchanged.

127. P2 - The pinned-state icon changed the pin button surface, making the state read as a filled action button instead of an icon-only state.
   - Fix: keep the pin button background transparent in the pinned state, retain the shared hover surface, and rotate the existing Fluent pin glyph to the upright orientation while preserving the normal/hover/press motion.

128. P2 - Keyboard focus could leave a visible outline around the pinned icon, which looked like an unintended button border.
   - Fix: suppress the focus outline for the pin control so its pinned state is communicated only by the upright glyph.

129. P2 - The footer settings button used a filled action background with a white icon on hover, unlike the other small icon controls.
   - Fix: use the shared count-surface hover background and semantic action icon color, yielding a gray background with a black icon in light mode and the standard blue icon in dark mode.

final result: passed
