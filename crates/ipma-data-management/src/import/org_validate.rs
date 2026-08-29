//! 组织模板/组织导入行的结构校验（与 API 端 `ipma-organization` 的
//! `validate_levels_mapping`、`resolve_type_name` 语义对齐）。
//!
//! CSV 导入路径无法直接依赖 ipma-organization（依赖方向不允许），
//! 本模块按相同规则复刻校验：levels 必须是对象、类型数量与命名受限、
//! 子级类型必须已定义、有且仅有一个根类型、无环（全起点 DFS 染色）、
//! 深度不超上限；organizations.type_path 必须能被模板 levels 解析。
//! 两边规则如需调整必须同步修改。

use serde_json::Value;

/// 组织层级最大深度（与 API 端 `MAX_ORG_DEPTH` 一致：根为第 1 层）
pub const MAX_ORG_DEPTH: usize = 10;

/// 类型 key 允许的字符：Unicode 字母/数字（含中文）、下划线、连字符
/// （与 API 端 `is_valid_type_char` 一致，拒绝 HTML 元字符）
fn is_valid_type_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// 校验 levels 映射结构（与 API 端 `validate_levels_mapping` 语义一致）。
/// 返回 Err 时携带既有的 `server.org_template.validation.*` 消息 key，
/// 由前端 i18n 渲染，与 API 端校验失败的提示完全一致。
pub fn validate_levels_mapping(levels: &Value) -> Result<(), ipma_common::AppMessage> {
    use ipma_common::msg;

    let levels_map = levels
        .as_object()
        .ok_or_else(|| msg("server.org_template.validation.levels_must_be_object"))?;

    if levels_map.is_empty() {
        return Err(msg("server.org_template.validation.levels_empty"));
    }

    if levels_map.len() > 50 {
        return Err(msg("server.org_template.validation.levels_too_many_types"));
    }

    let mut all_child_types: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (key, value) in levels_map {
        if key.trim().is_empty() {
            return Err(msg("server.org_template.validation.type_name_required"));
        }
        if key.len() > 50 {
            return Err(msg("server.org_template.validation.type_name_too_long").with("name", key));
        }
        if let Some(invalid) = key.chars().find(|c| !is_valid_type_char(*c)) {
            return Err(msg("server.org_template.validation.type_name_invalid")
                .with("name", key)
                .with("char", invalid));
        }

        let children = value.as_array().ok_or_else(|| {
            msg("server.org_template.validation.children_not_array").with("name", key)
        })?;

        let mut seen_in_this_key: std::collections::HashSet<&str> =
            std::collections::HashSet::new();
        for (idx, child) in children.iter().enumerate() {
            let child_str = child.as_str().ok_or_else(|| {
                msg("server.org_template.validation.child_not_string")
                    .with("name", key)
                    .with("index", idx)
            })?;
            if child_str.trim().is_empty() {
                return Err(msg("server.org_template.validation.child_empty")
                    .with("name", key)
                    .with("index", idx));
            }
            if child_str.len() > 50 {
                return Err(msg("server.org_template.validation.child_too_long")
                    .with("name", key)
                    .with("index", idx));
            }
            if !seen_in_this_key.insert(child_str) {
                return Err(msg("server.org_template.validation.child_duplicate")
                    .with("name", key)
                    .with("child", child_str));
            }
            all_child_types.insert(child_str.to_string());
        }
    }

    // 所有子类型必须在映射中定义
    for child_type in &all_child_types {
        if !levels_map.contains_key(child_type) {
            return Err(
                msg("server.org_template.validation.child_type_undefined").with("type", child_type)
            );
        }
    }

    // 必须有且仅有一个根类型（根唯一）
    let root_types: Vec<&String> = levels_map
        .keys()
        .filter(|k| !all_child_types.contains(*k))
        .collect();
    let root_type = match root_types.len() {
        1 => root_types[0].clone(),
        0 => return Err(msg("server.org_template.validation.root_type_missing")),
        _ => {
            let types = root_types
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("、");
            return Err(
                msg("server.org_template.validation.root_type_multiple").with("types", types)
            );
        }
    };

    // 环检测（DFS 染色法，全起点覆盖游离环，与 API 端一致）
    let mut visited: std::collections::HashMap<&str, u8> = std::collections::HashMap::new();
    fn has_cycle<'a>(
        node: &'a str,
        levels_map: &'a serde_json::Map<String, Value>,
        visited: &mut std::collections::HashMap<&'a str, u8>,
    ) -> bool {
        match visited.get(node) {
            Some(&2) => return false,
            Some(&1) => return true,
            _ => {}
        }
        visited.insert(node, 1);
        if let Some(children) = levels_map.get(node).and_then(|v| v.as_array()) {
            for child in children {
                if let Some(child_str) = child.as_str()
                    && let Some(child_key) = levels_map.keys().find(|k| k.as_str() == child_str)
                    && has_cycle(child_key, levels_map, visited)
                {
                    return true;
                }
            }
        }
        visited.insert(node, 2);
        false
    }
    for key in levels_map.keys() {
        if has_cycle(key, levels_map, &mut visited) {
            return Err(msg("server.org_template.validation.levels_cycle"));
        }
    }

    // 层级深度不得超过组织创建上限（根为第 1 层）
    let mut max_depth = 0usize;
    fn walk_depth(
        node: &str,
        levels_map: &serde_json::Map<String, Value>,
        depth: usize,
        max_depth: &mut usize,
    ) {
        *max_depth = (*max_depth).max(depth);
        let Some(children) = levels_map.get(node).and_then(|v| v.as_array()) else {
            return;
        };
        for child in children {
            if let Some(child_str) = child.as_str() {
                walk_depth(child_str, levels_map, depth + 1, max_depth);
            }
        }
    }
    walk_depth(&root_type, levels_map, 1, &mut max_depth);
    if max_depth > MAX_ORG_DEPTH {
        return Err(msg("server.org_template.validation.levels_depth_exceeded")
            .with("depth", max_depth)
            .with("max", MAX_ORG_DEPTH));
    }

    Ok(())
}

/// 从 levels 映射中找根节点 key（不被任何子级列表引用的类型）
fn find_root_in_levels(levels_map: &serde_json::Map<String, Value>) -> Option<String> {
    let mut child_types: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for value in levels_map.values() {
        if let Some(arr) = value.as_array() {
            for child in arr {
                if let Some(s) = child.as_str() {
                    child_types.insert(s);
                }
            }
        }
    }
    levels_map
        .keys()
        .find(|key| !child_types.contains(key.as_str()))
        .cloned()
}

/// 校验 type_path 能否被给定模板 levels 解析（与 API 端
/// `resolve_type_name` 的导航规则一致：首段必须为根锚点 0，
/// 后续段逐级索引子级类型）。可解析返回 Ok(())，否则 Err(())。
pub fn type_path_resolvable(levels: &Value, type_path: &str) -> bool {
    let Some(levels_map) = levels.as_object() else {
        return false;
    };
    let Ok(indices) = type_path
        .split('.')
        .map(|s| s.parse::<usize>())
        .collect::<Result<Vec<_>, _>>()
    else {
        return false;
    };
    if indices.is_empty() || indices[0] != 0 {
        return false;
    }
    let Some(root) = find_root_in_levels(levels_map) else {
        return false;
    };
    if indices.len() == 1 {
        return true;
    }
    let mut current_type = root;
    for &idx in &indices[1..] {
        let Some(children) = levels_map.get(&current_type).and_then(|v| v.as_array()) else {
            return false;
        };
        match children.get(idx).and_then(|v| v.as_str()) {
            Some(next) => current_type = next.to_string(),
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn levels_合法结构通过() {
        let levels = json!({
            "总部": ["a1公司", "b1公司"],
            "a1公司": [],
            "b1公司": []
        });
        assert!(validate_levels_mapping(&levels).is_ok());
    }

    #[test]
    fn levels_根唯一与环检测() {
        // 双根拒绝
        let levels = json!({ "a": [], "b": [] });
        let err = validate_levels_mapping(&levels).unwrap_err();
        assert!(err.key().contains("root_type_multiple"));

        // 游离环拒绝
        let levels = json!({ "公司": ["部门"], "部门": [], "x": ["y"], "y": ["x"] });
        let err = validate_levels_mapping(&levels).unwrap_err();
        assert!(err.key().contains("levels_cycle"));
    }

    #[test]
    fn levels_深度上限() {
        // 构造深度 MAX_ORG_DEPTH + 1 的线性链
        let mut map = serde_json::Map::new();
        for i in 0..=MAX_ORG_DEPTH {
            let key = format!("t{i}");
            let children = if i < MAX_ORG_DEPTH {
                vec![json!(format!("t{}", i + 1))]
            } else {
                vec![]
            };
            map.insert(key, Value::Array(children));
        }
        let err = validate_levels_mapping(&Value::Object(map)).unwrap_err();
        assert!(err.key().contains("levels_depth_exceeded"));
    }

    #[test]
    fn type_path_解析判定() {
        let levels = json!({
            "总部": ["a1公司", "b1公司"],
            "a1公司": ["部门"],
            "b1公司": [],
            "部门": []
        });
        assert!(type_path_resolvable(&levels, "0"));
        assert!(type_path_resolvable(&levels, "0.1"));
        assert!(type_path_resolvable(&levels, "0.0.0"));
        // 根锚点非 0、越界、非数字路径均不可解析
        assert!(!type_path_resolvable(&levels, "1"));
        assert!(!type_path_resolvable(&levels, "0.9"));
        assert!(!type_path_resolvable(&levels, "0.x"));
        assert!(!type_path_resolvable(&levels, ""));
        assert!(!type_path_resolvable(&json!("not-object"), "0"));
    }
}
