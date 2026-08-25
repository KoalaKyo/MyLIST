# MyLIST 语言包说明

本目录是 MyLIST 的语言包模板来源。程序首次运行外置语言包版本时，会把这些文件复制到当前 Windows 用户的可编辑目录；以后界面从外置文件读取，不需要重新打包。

## 文件

- `zh-CN.ts`：完整简体中文基线与全部键。
- `en.ts`、`de.ts`、`fr.ts`、`it.ts`、`es.ts`、`ja.ts`、`zh-TW.ts`：对应语言文件。
- `index.ts`：类型、格式化和运行时切换；不填写具体翻译。

## 实际翻译文件位置

安装/启动外置语言包版本后，实际生效的文件在：

```text
%LOCALAPPDATA%\com.mylist.desktop\locales\
```

例如英语文件是 `en.ts`。程序只会在文件不存在时创建模板，绝不会覆盖已编辑的外置文件。编辑完成后，在设置中切换到该语言即可即时重新读取并生效；若当前已经是该语言，切换到任一其他语言后再切回来即可。

## 翻译方式

未开始翻译的外置语言文件会自动生成一份完整的简体中文基线（每个键独占一行）；翻译完成前也能正常显示。损坏、无法读取或缺少必要内容时，程序才安全回退为英文。翻译某个语言时，只在该文件的 `messages` 中按模块逐条修改同名键，例如：

```ts
messages: {
  // 应用与窗口
  "app.settings": "Settings",
  "app.back": "Back",
}
```

每一条翻译独占一行，并使用与 `zh-CN.ts` 相同的模块分段。不要修改键名、插值名（如 `{count}`）或 locale 值。外置文件损坏、不完整或无法读取时，程序会安全回退为英文，不会显示空白界面。

## 默认分类

`个人、团队、工作、生活、出行、财务、学习、其他` 使用稳定内部键，不直接把显示名称当作业务数据。请同步翻译以下八个键：

```ts
"category.default.personal"
"category.default.team"
"category.default.work"
"category.default.life"
"category.default.travel"
"category.default.finance"
"category.default.study"
"category.default.other"
```

切换语言时，这些系统默认分类会自动显示为对应语言。用户新建的分类、或用户手动改名后的默认分类，始终保留用户输入的原文，不会被语言切换改写。

## 验证

语言选择立即写入本机设置，托盘菜单和文件选择器标签由当前语言包同步更新。只有修改应用功能代码或新增语言键时，才需要重新运行构建。
