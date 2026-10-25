/**
 * 自检脚本的 ESM 解析钩子：为无扩展名的相对导入补 `.ts`。
 *
 * Node 原生类型擦除要求显式文件扩展名，而源码按 bundler 约定写无扩展名导入
 * （`from "./geometry"`）。本钩子只在解析失败时补 `.ts` 重试，不改动源码，
 * 让 `scripts/*.selfcheck.ts` 能直接运行。
 *
 * 用法：`node --import ./scripts/ts-resolve-hook.mjs scripts/xxx.selfcheck.ts`
 */
import { registerHooks } from "node:module";

/** 是否已是带扩展名的说明符 */
const HAS_EXTENSION = /\.[cm]?[jt]sx?$/;

/**
 * 解析相对说明符：无扩展名时补 `.ts` 重试，仍失败则交回默认解析
 * （默认解析会给出准确的错误信息）。
 */
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith(".") && !HAS_EXTENSION.test(specifier)) {
      try {
        return nextResolve(`${specifier}.ts`, context);
      } catch {
        // 落到默认解析：文件确实不存在，由 Node 报告原始错误
      }
    }
    return nextResolve(specifier, context);
  },
});
