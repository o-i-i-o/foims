//! 组织层级模板（levels / type_path）校验与解析的唯一权威实现。
//!
//! levels 是「类型 → 子类型列表」的树模板，其结构契约（对象/数量/命名/
//! 子级已定义/唯一根/无环/深度上限）与 type_path 导航规则（根锚点 0 +
//! 逐级索引子级）同时被组织 API（`foims-organization`）与 CSV 导入
//! （`foims-data-management`）消费。此前两侧各有一份复刻实现并靠注释约定
//! 「修改需同步」，本模块将其收敛为单点：任何规则调整只改这里，
//! 全仓库行为自动一致。
//!
//! 错误一律返回既有的 i18n 消息 key（`AppMessage`），由调用方按自身语义
//! 包装（API 端 `AppError::Validation`/`Internal`，导入端 `DataError::Validation`），
//! 前端提示保持逐字不变。

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::msg;
use crate::{AppError, AppMessage};

/// 组织层级最大深度（根为第 1 层），模板校验与组织节点创建共用
pub const MAX_ORG_DEPTH: usize = 10;

/// 类型 key 允许的字符：Unicode 字母/数字（含中文）、下划线、连字符。
/// 拒绝 HTML 元字符，防止类型名注入前端 innerHTML 渲染。
fn is_valid_type_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// 从 levels 映射中找根节点 key（不被任何子级列表引用的类型）。
/// 结构可能非法（零根/多根），故返回 Option，由调用方决定报错语义。
pub fn find_root(levels_map: &Map<String, Value>) -> Option<&str> {
    let mut child_types: HashSet<&str> = HashSet::new();
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
        .map(String::as_str)
}

/// 校验 levels 映射结构并返回根类型名。
/// levels 格式: `{ "type_a": ["type_b"], "type_b": ["type_c", "type_d"], ... }`
pub fn validate_levels_mapping(levels: &Value) -> Result<String, AppMessage> {
    let levels_map = levels
        .as_object()
        .ok_or_else(|| msg("server.org_template.validation.levels_must_be_object"))?;

    if levels_map.is_empty() {
        return Err(msg("server.org_template.validation.levels_empty"));
    }

    if levels_map.len() > 50 {
        return Err(msg("server.org_template.validation.levels_too_many_types"));
    }

    let mut all_child_types: HashSet<String> = HashSet::new();

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

        let mut seen_in_this_key: HashSet<&str> = HashSet::new();
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

    for child_type in &all_child_types {
        if !levels_map.contains_key(child_type) {
            return Err(
                msg("server.org_template.validation.child_type_undefined").with("type", child_type)
            );
        }
    }

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

    // 环检测（DFS 染色法，全起点覆盖游离环）：必须从全部节点出发，仅从根
    // 出发时游离于根可达范围之外的环（非根类型互相引用成环）会漏检通过
    // 校验；已访问标记跨起点复用（2=已完成），整体仍为 O(节点数 + 边数)
    let mut visited: HashMap<&str, u8> = HashMap::new();
    for key in levels_map.keys() {
        if has_cycle(key, levels_map, &mut visited) {
            return Err(msg("server.org_template.validation.levels_cycle"));
        }
    }

    // 层级深度不得超过组织创建上限（根为第 1 层），否则模板能建、组织节点建不全
    let mut max_depth = 0usize;
    walk_depth(&root_type, levels_map, 1, &mut max_depth);
    if max_depth > MAX_ORG_DEPTH {
        return Err(msg("server.org_template.validation.levels_depth_exceeded")
            .with("depth", max_depth)
            .with("max", MAX_ORG_DEPTH));
    }

    Ok(root_type)
}

fn has_cycle<'a>(
    node: &'a str,
    levels_map: &'a Map<String, Value>,
    visited: &mut HashMap<&'a str, u8>,
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

fn walk_depth(node: &str, levels_map: &Map<String, Value>, depth: usize, max_depth: &mut usize) {
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

/// 从模板的 levels JSON 中，根据 type_path 解析出类型名称。
///
/// type_path 格式为点分隔的索引路径，如 "0"、"0.1"、"0.0.2"；
/// 首段是根锚点，必须为 0（防止 "9.1" 之类的畸形路径被静默当作 "0.1" 解析），
/// 后续段逐级索引该类型的子级列表。
///
/// 错误语义按性质分层：levels 非对象/断链属数据损坏（Internal），
/// 路径格式/越界属用户输入问题（Validation）。CSV 导入侧只需判定
/// 「能否解析」时以 `is_ok()` 消费本函数，与 API 端共用同一导航规则。
pub fn resolve_type_name(levels: &Value, type_path: &str) -> Result<String, AppError> {
    let levels_map = levels
        .as_object()
        .ok_or_else(|| AppError::Internal(msg("server.org_template.levels_format_invalid")))?;

    if type_path.is_empty() {
        return Err(AppError::Validation(
            msg("server.organization.validation.type_path_required").with("path", type_path),
        ));
    }

    let indices: Vec<usize> = type_path
        .split('.')
        .map(|s| {
            s.parse::<usize>().map_err(|_| {
                AppError::Validation(
                    msg("server.organization.validation.type_path_format").with("path", type_path),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    if indices[0] != 0 {
        return Err(AppError::Validation(
            msg("server.organization.validation.type_path_root_anchor").with("path", type_path),
        ));
    }

    let root = find_root(levels_map)
        .ok_or_else(|| AppError::Internal(msg("server.org_template.root_not_found")))?;

    // "0" → 根节点；"0.x" → 根的第 x 个子级；"0.x.y" → 再下一级
    if indices.len() == 1 {
        return Ok(root.to_string());
    }

    let mut current_type = root;
    for &idx in &indices[1..] {
        let children = levels_map
            .get(current_type)
            .and_then(|v| v.as_array())
            .ok_or_else(|| AppError::Internal(msg("server.org_template.levels_format_invalid")))?;

        current_type = children.get(idx).and_then(|v| v.as_str()).ok_or_else(|| {
            AppError::Validation(
                msg("server.organization.validation.type_path_out_of_range")
                    .with("path", type_path)
                    .with("index", idx),
            )
        })?;
    }

    Ok(current_type.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 构造一条深度为 depth 的线性链 levels
    fn chain_levels(depth: usize) -> Value {
        let mut map = Map::new();
        for i in 0..depth {
            let key = format!("t{i}");
            let children = if i + 1 < depth {
                vec![json!(format!("t{}", i + 1))]
            } else {
                vec![]
            };
            map.insert(key, Value::Array(children));
        }
        Value::Object(map)
    }

    #[test]
    fn levels_合法结构通过() {
        let levels = json!({
            "总部": ["a1公司", "b1公司"],
            "a1公司": [],
            "b1公司": []
        });
        assert_eq!(validate_levels_mapping(&levels).unwrap(), "总部");
    }

    #[test]
    fn levels_根唯一与环检测() {
        // 双根拒绝
        let levels = json!({ "a": [], "b": [] });
        let err = validate_levels_mapping(&levels).unwrap_err();
        assert!(err.key().contains("root_type_multiple"));

        // 游离环拒绝（仅从根出发的 DFS 会漏检该类环）
        let levels = json!({ "公司": ["部门"], "部门": [], "x": ["y"], "y": ["x"] });
        let err = validate_levels_mapping(&levels).unwrap_err();
        assert!(err.key().contains("levels_cycle"));
    }

    #[test]
    fn levels_深度上限() {
        assert!(validate_levels_mapping(&chain_levels(MAX_ORG_DEPTH)).is_ok());
        let err = validate_levels_mapping(&chain_levels(MAX_ORG_DEPTH + 1)).unwrap_err();
        assert!(err.key().contains("levels_depth_exceeded"));
    }

    #[test]
    fn type_path_解析成功路径() {
        let levels = json!({
            "总部": ["a1公司", "b1公司"],
            "a1公司": ["部门"],
            "b1公司": [],
            "部门": []
        });
        assert_eq!(resolve_type_name(&levels, "0").unwrap(), "总部");
        assert_eq!(resolve_type_name(&levels, "0.0").unwrap(), "a1公司");
        assert_eq!(resolve_type_name(&levels, "0.1").unwrap(), "b1公司");
        assert_eq!(resolve_type_name(&levels, "0.0.0").unwrap(), "部门");
    }

    #[test]
    fn type_path_非法路径拒绝() {
        let levels = json!({
            "总部": ["a1公司", "b1公司"],
            "a1公司": [],
            "b1公司": []
        });
        // 根锚点非 0、越界、非数字、空路径、levels 非对象均拒绝
        assert!(resolve_type_name(&levels, "1.0").is_err());
        assert!(resolve_type_name(&levels, "0.9").is_err());
        assert!(resolve_type_name(&levels, "0.x").is_err());
        assert!(resolve_type_name(&levels, "").is_err());
        assert!(resolve_type_name(&json!("not-object"), "0").is_err());
    }
}
