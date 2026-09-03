//! 前端资源一致性校验（无 Node 工具链，随 cargo test 运行）。
//!
//! 防止"改 HTML 忘改 JS / 改 JS 忘同步注册表"式静默断裂 —— 此类问题在
//! 运行期的表现是 elementCache 返回 null、回调表查不到即跳过，无任何报错：
//! 1. JS 引用的 DOM id 必须存在于任一 HTML 或 JS 模板字符串中的 id 定义
//! 2. MODULE_REGISTRY / MODAL_REGISTRY 指向的文件必须真实存在；
//!    注册表键必须与片段内的根 id 对应；data-modal-id 必须指向注册表键
//! 3. i18n：en/zh 键集合双向一致；JS 字面量 t("…") 与 HTML 的
//!    data-i18n* 属性引用的键必须存在
//! 4. 版本号：三个入口 HTML 的 ?v= 与 resourceLoader MODULE_VERSION 一致
//! 5. 动态加载的模块名（loadModule/getModule/lazyLoad/createCallback 及
//!    预载清单）必须在 MODULE_REGISTRY 中注册
//!
//! 动态拼接的 id / key（如 `"#" + tab + "-tab"`）无法静态解析，不在校验范围。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

fn web_static() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("web")
        .join("static")
}

fn read_file(rel: &[&str]) -> String {
    let mut path = web_static();
    for seg in rel {
        path.push(seg);
    }
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {:?} 失败: {}", path, e))
}

/// 递归收集目录下指定后缀的全部文件
fn collect_files(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("读取目录 {:?} 失败: {}", dir, e));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, ext, out);
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

fn extract_matches(pattern: &str, haystack: &str) -> BTreeSet<String> {
    let re = Regex::new(pattern).unwrap_or_else(|e| panic!("非法正则: {e}"));
    re.captures_iter(haystack)
        .map(|c| c[1].to_string())
        .collect()
}

fn all_html_files() -> Vec<PathBuf> {
    let mut html_files = vec![
        web_static().join("main.html"),
        web_static().join("index.html"),
        web_static().join("init_index.html"),
    ];
    collect_files(&web_static().join("modals"), "html", &mut html_files);
    html_files
}

fn all_js_files() -> Vec<PathBuf> {
    let mut js_files = Vec::new();
    collect_files(&web_static().join("js"), "js", &mut js_files);
    js_files
}

/// 扁平化 i18n JSON 为点分键集合
fn flatten_json_keys(value: &serde_json::Value, prefix: &str, out: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };
                flatten_json_keys(v, &key, out);
            }
        }
        _ => {
            out.insert(prefix.to_string());
        }
    }
}

#[test]
fn js_referenced_dom_ids_must_exist() {
    let html_files = all_html_files();
    let js_files = all_js_files();

    // 1. 收集全部 id 定义：HTML 静态文件 + JS 模板字符串中的 id="…"
    let id_re = Regex::new(r#"id="([A-Za-z0-9_-]+)""#).unwrap_or_else(|e| panic!("非法正则: {e}"));
    let mut defined: BTreeSet<String> = BTreeSet::new();
    for path in &html_files {
        let content = fs::read_to_string(path).unwrap_or_default();
        for cap in id_re.captures_iter(&content) {
            defined.insert(cap[1].to_string());
        }
    }
    for path in &js_files {
        let content = fs::read_to_string(path).unwrap_or_default();
        for cap in id_re.captures_iter(&content) {
            defined.insert(cap[1].to_string());
        }
        // JS 动态创建的元素：el.id = "x" / setAttribute("id", "x")
        for pat in [
            r#"\.id = "([A-Za-z0-9_-]+)""#,
            r#"setAttribute\("id", "([A-Za-z0-9_-]+)""#,
        ] {
            defined.extend(extract_matches(pat, &content));
        }
    }

    // 2. 收集 JS 中的 id 引用（静态字符串字面量，双引号与单引号对称覆盖）
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    let ref_patterns = [
        r#"getElementById\(\s*"([A-Za-z0-9_-]+)""#,
        r#"getElementById\(\s*'([A-Za-z0-9_-]+)'"#,
        r#"elementCache\.(?:get|getValue|setValue|refresh)\(\s*"([A-Za-z0-9_-]+)""#,
        r#"elementCache\.(?:get|getValue|setValue|refresh)\(\s*'([A-Za-z0-9_-]+)'"#,
        r#"getElementValue\(\s*"([A-Za-z0-9_-]+)""#,
        r#"getElementValue\(\s*'([A-Za-z0-9_-]+)'"#,
        // 纯 #id 形态的选择器字符串（querySelector("#x")、setText("#x") 等）
        r##""#([A-Za-z0-9_-]+)""##,
        r"'#([A-Za-z0-9_-]+)'",
    ];
    for path in &js_files {
        let content = fs::read_to_string(path).unwrap_or_default();
        for pat in ref_patterns {
            referenced.extend(extract_matches(pat, &content));
        }
    }

    // 排除十六进制色值（"#fff"、"#0d47a1" 等字符串与 #id 选择器同形）
    let hex_color = Regex::new("^[0-9a-fA-F]{3,8}$").unwrap_or_else(|e| panic!("非法正则: {e}"));
    referenced.retain(|id| !hex_color.is_match(id));

    let dangling: Vec<String> = referenced.difference(&defined).cloned().collect();
    assert!(
        dangling.is_empty(),
        "JS 引用了不存在的 DOM id（运行期将静默失效）：\n{:?}\n\
         请同步修复 HTML/JS，或确认为动态生成的 id 后加入测试豁免清单",
        dangling
    );
}

#[test]
fn registry_files_must_exist_and_keys_match_root_ids() {
    let static_root = web_static();

    let modal_loader = read_file(&["js", "utils", "modalLoader.js"]);
    let modal_registry_re = Regex::new(r#""([\w-]+)":\s*"(/static/modals/[^"]+)""#)
        .unwrap_or_else(|e| panic!("非法正则: {e}"));
    for cap in modal_registry_re.captures_iter(&modal_loader) {
        let key = cap[1].to_string();
        let path = cap[2].to_string();
        let file = static_root.join(path.trim_start_matches("/static/"));
        assert!(file.is_file(), "MODAL_REGISTRY 指向的文件不存在: {}", path);
        // 注册表键必须与片段内的元素 id 对应，否则 openModal/closeModal 找不到根
        let content = fs::read_to_string(&file).unwrap_or_default();
        assert!(
            content.contains(&format!("id=\"{key}\"")),
            "MODAL_REGISTRY 键 {key} 在 {} 中没有对应的 id=\"{key}\" 定义",
            file.display()
        );
    }

    let resource_loader = read_file(&["js", "utils", "resourceLoader.js"]);
    for path in extract_matches(r#":\s*"(/static/js/[^"]+\.js)""#, &resource_loader) {
        let file = static_root.join(path.trim_start_matches("/static/"));
        assert!(file.is_file(), "MODULE_REGISTRY 指向的文件不存在: {}", path);
    }

    // 所有模态片段的 data-modal-id（关闭按钮委托的依据）必须指向注册表键
    let registry_keys: BTreeSet<String> = modal_registry_re
        .captures_iter(&modal_loader)
        .map(|c| c[1].to_string())
        .collect();
    for path in all_html_files() {
        let content = fs::read_to_string(&path).unwrap_or_default();
        for id in extract_matches(r#"data-modal-id="([\w-]+)""#, &content) {
            assert!(
                registry_keys.contains(&id),
                "{} 中 data-modal-id=\"{id}\" 不在 MODAL_REGISTRY 中，关闭按钮将失效",
                path.display()
            );
        }
    }
}

#[test]
fn i18n_keys_must_be_consistent_and_referenced_keys_exist() {
    let zh: serde_json::Value = serde_json::from_str(&read_file(&["i18n", "zh.json"]))
        .unwrap_or_else(|e| panic!("zh.json 解析失败: {e}"));
    let en: serde_json::Value = serde_json::from_str(&read_file(&["i18n", "en.json"]))
        .unwrap_or_else(|e| panic!("en.json 解析失败: {e}"));

    let mut zh_keys = BTreeSet::new();
    flatten_json_keys(&zh, "", &mut zh_keys);
    let mut en_keys = BTreeSet::new();
    flatten_json_keys(&en, "", &mut en_keys);

    let only_zh: Vec<String> = zh_keys.difference(&en_keys).cloned().collect();
    let only_en: Vec<String> = en_keys.difference(&zh_keys).cloned().collect();
    assert!(
        only_zh.is_empty() && only_en.is_empty(),
        "i18n 键漂移：仅在 zh = {:?}，仅在 en = {:?}",
        only_zh,
        only_en
    );

    let mut referenced: BTreeSet<String> = BTreeSet::new();

    // JS 中 t("字面量") 引用的键
    for path in all_js_files() {
        let content = fs::read_to_string(path).unwrap_or_default();
        referenced.extend(extract_matches(r#"\bt\(\s*"([A-Za-z0-9_.]+)""#, &content));
        referenced.extend(extract_matches(r"\bt\(\s*'([A-Za-z0-9_.]+)'", &content));
    }

    // HTML 中 data-i18n / -placeholder / -aria-label / -title 属性引用的键
    // （缺失时 UI 直接显示英文兜底文本或裸键名，此前无任何校验）
    for path in all_html_files() {
        let content = fs::read_to_string(path).unwrap_or_default();
        for pat in [
            r#"data-i18n="([A-Za-z0-9_.-]+)""#,
            r#"data-i18n-placeholder="([A-Za-z0-9_.-]+)""#,
            r#"data-i18n-aria-label="([A-Za-z0-9_.-]+)""#,
            r#"data-i18n-title="([A-Za-z0-9_.-]+)""#,
        ] {
            referenced.extend(extract_matches(pat, &content));
        }
    }

    // 动态拼接的键前缀（如 t("prefix." + key)）无法静态解析，跳过
    referenced.retain(|k| !k.ends_with('.'));

    let missing: Vec<String> = referenced.difference(&zh_keys).cloned().collect();
    assert!(
        missing.is_empty(),
        "JS t() / HTML data-i18n* 引用了不存在的 i18n 键：\n{:?}\n\
         （若为动态拼接键请改写或加入豁免；HTML 属性请改为实际存在的键）",
        missing
    );
}

#[test]
fn dynamically_loaded_module_names_must_be_registered() {
    let resource_loader = read_file(&["js", "utils", "resourceLoader.js"]);
    let registry: BTreeSet<String> =
        extract_matches(r##"(?m)^\s{2}([\w-]+):\s*"/static/js/"##, &resource_loader);

    let mut used: BTreeSet<String> = BTreeSet::new();
    for path in all_js_files() {
        let content = fs::read_to_string(path).unwrap_or_default();
        for pat in [
            r#"loadModule\(\s*"([\w-]+)""#,
            r#"getModule\(\s*"([\w-]+)""#,
            r#"lazyLoad\(\s*"([\w-]+)""#,
            r#"createCallback\(\s*"([\w-]+)""#,
        ] {
            used.extend(extract_matches(pat, &content));
        }
        // 预载清单数组：PRELOAD_MODULES = ["a", "b"] / schedulePreload([\n "a",...])
        for pat in [
            r#"PRELOAD_MODULES = \[([^\]]*)\]"#,
            r#"schedulePreload\(\s*\[([^\]]*)\]"#,
        ] {
            let re = Regex::new(pat).unwrap_or_else(|e| panic!("非法正则: {e}"));
            for cap in re.captures_iter(&content) {
                used.extend(extract_matches(r#""([\w-]+)""#, &cap[1]));
            }
        }
    }
    // 配置表形如 module: "name" 的间接引用（resourceTabs TAB_CONFIG、
    // eventManager EDIT/DELETE_FUNCTIONS）
    used.extend(extract_matches(
        r#"module:\s*"([\w-]+)""#,
        &read_file(&["js", "modules", "resourceTabs.js"]),
    ));
    used.extend(extract_matches(
        r#"module:\s*"([\w-]+)""#,
        &read_file(&["js", "modules", "eventManager.js"]),
    ));

    let unregistered: Vec<String> = used.difference(&registry).cloned().collect();
    assert!(
        unregistered.is_empty(),
        "动态加载的模块名未注册进 MODULE_REGISTRY：\n{:?}\n\
         （loadModule 运行期会抛 Module not found）",
        unregistered
    );
}

#[test]
fn asset_versions_must_be_uniform() {
    let module_version = extract_matches(
        r#"MODULE_VERSION = "([^"]+)""#,
        &read_file(&["js", "utils", "resourceLoader.js"]),
    )
    .into_iter()
    .next()
    .unwrap_or_else(|| panic!("resourceLoader.js 中未找到 MODULE_VERSION"));

    for entry in ["main.html", "index.html", "init_index.html"] {
        let content = read_file(&[entry]);
        let versions = extract_matches(r#"[?&]v=([A-Za-z0-9.]+)"#, &content);
        assert!(
            !versions.is_empty(),
            "{} 中未找到任何 ?v= 版本号引用",
            entry
        );
        assert!(
            versions.len() == 1,
            "{} 内版本号不统一：{:?}",
            entry,
            versions
        );
        assert_eq!(
            versions.iter().next().map(String::as_str),
            Some(module_version.as_str()),
            "{} 的 ?v= 与 resourceLoader MODULE_VERSION({}) 不一致 —— \
             改动静态资源后需同步 bump 全部入口",
            entry,
            module_version
        );
    }
}
