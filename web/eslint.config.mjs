// ESLint 9 平面配置：代码质量归 ESLint，格式归 Prettier（见 docs/code-style.md 3.6）
// 插件：import（导入规范/死链）、sonarjs（复杂度/潜在 bug）、prettier（关闭格式冲突）
import js from "@eslint/js";
import eslintPluginImport from "eslint-plugin-import";
import sonarjs from "eslint-plugin-sonarjs";
import eslintConfigPrettier from "eslint-config-prettier";
import globals from "globals";
import { FlatCompat } from "@eslint/eslintrc";

// eslint-plugin-import 的 recommended 仍是 eslintrc 格式，经 FlatCompat 转平面配置
const compat = new FlatCompat({ baseDirectory: import.meta.dirname });

export default [
  {
    ignores: ["node_modules/", "coverage/"]
  },
  js.configs.recommended,
  ...compat.config(eslintPluginImport.configs.recommended),
  sonarjs.configs.recommended,
  {
    languageOptions: {
      ecmaVersion: 2026,
      sourceType: "module",
      globals: {
        ...globals.browser
      }
    },
    rules: {
      // —— 原有规则基线（原 .eslintrc.json，2026-08 迁移） ——
      "no-console": "off",
      "no-unused-vars": [
        "error",
        {
          argsIgnorePattern: "^_"
        }
      ],
      "prefer-const": "error",
      "no-var": "error",
      eqeqeq: ["error", "always", { null: "ignore" }],
      "prefer-arrow-callback": "error",
      "prefer-template": "error",
      "object-shorthand": ["error", "always"],
      "no-duplicate-imports": "error",
      "no-useless-return": "error",
      "no-else-return": ["error", { allowElseIf: false }],

      // —— sonarjs 阈值校准（全站 UI 模块体量较大，保持推荐集但放宽经验阈值） ——
      // 认知复杂度 15→40：表格渲染/表单校验分支天然密集，40 以上才值得拆函数
      "sonarjs/cognitive-complexity": ["error", 40],
      // 重复字符串 3→6：模板串中的 CSS 类名/i18n 键重复属正常，6 以上提示抽常量。
      // ignoreStrings 为逗号分隔的精确白名单（非正则）：DOM 元素 id 必须保持字面量，
      // tests/frontend_consistency.rs 靠正则匹配 elementCache.get("…")/getElementById("…")
      // 做悬空校验；application/json 为默认豁免项，透传时需显式保留
      "sonarjs/no-duplicate-string": [
        "error",
        {
          threshold: 6,
          ignoreStrings:
            "application/json,device-workstation-id,device-position-id,device-template-id,device-room-id,device-cabinet-id"
        }
      ]
    }
  },
  {
    // Jest 测试文件：补充 jest 环境全局变量
    files: ["tests/**/*.js"],
    languageOptions: {
      globals: {
        ...globals.jest
      }
    },
    rules: {
      // 测试夹具中的 IP/CIDR 字面量即被测数据，天然"硬编码"
      "sonarjs/no-hardcoded-ip": "off"
    }
  },
  {
    // Node 环境脚本：lint 配置自身
    files: ["eslint.config.mjs", "jest.config.mjs", "stylelint.config.mjs"],
    languageOptions: {
      globals: {
        ...globals.node
      }
    }
  },
  eslintConfigPrettier
];
