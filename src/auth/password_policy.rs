//! 等保三级密码策略：复杂度、有效期与历史重复检查。
//!
//! 策略存于 `system_configs`（config_type='password_policy'），
//! 未配置时按等保三级默认值执行：长度 ≥8、含大小写字母与数字、
//! 有效期 90 天、不得与最近 5 次历史密码重复。

use chrono::Utc;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use ipma_common::log_warn;
use ipma_common::{AppError, msg};

/// 密码策略配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PasswordPolicy {
    pub min_length: i32,
    pub require_upper: bool,
    pub require_lower: bool,
    pub require_digit: bool,
    pub require_special: bool,
    /// 密码有效期（天），0 表示不启用
    pub expiry_days: i32,
    /// 历史密码重复检查条数，0 表示不检查
    pub history_count: i32,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            min_length: 8,
            require_upper: true,
            require_lower: true,
            require_digit: true,
            require_special: false,
            expiry_days: 90,
            history_count: 5,
        }
    }
}

/// 从 system_configs 读取策略；未配置或解析失败时回落默认值
pub async fn load(pool: &PgPool) -> PasswordPolicy {
    let mut policy = PasswordPolicy::default();
    let Ok(rows) =
        sqlx::query("SELECT key, value FROM system_configs WHERE config_type = 'password_policy'")
            .fetch_all(pool)
            .await
    else {
        return policy;
    };

    for row in rows {
        let key: String = row.get("key");
        let value: Option<String> = row.get("value");
        let Some(value) = value else { continue };
        let apply_bool = |target: &mut bool, v: &str| {
            if let Ok(parsed) = v.parse::<bool>() {
                *target = parsed;
            }
        };
        let apply_i32 = |target: &mut i32, v: &str| {
            if let Ok(parsed) = v.parse::<i32>() {
                *target = parsed.clamp(0, 36500);
            }
        };
        match key.as_str() {
            "min_length" => apply_i32(&mut policy.min_length, &value),
            "expiry_days" => apply_i32(&mut policy.expiry_days, &value),
            "history_count" => apply_i32(&mut policy.history_count, &value),
            "require_upper" => apply_bool(&mut policy.require_upper, &value),
            "require_lower" => apply_bool(&mut policy.require_lower, &value),
            "require_digit" => apply_bool(&mut policy.require_digit, &value),
            "require_special" => apply_bool(&mut policy.require_special, &value),
            _ => {}
        }
    }
    // 下限保护：无论配置如何，长度不得低于 8（等保三级底线）
    policy.min_length = policy.min_length.max(8);
    policy
}

/// 保存策略到 system_configs
pub async fn save(pool: &PgPool, policy: &PasswordPolicy) -> Result<(), AppError> {
    let mut policy = policy.clone();
    policy.min_length = policy.min_length.max(8);
    policy.expiry_days = policy.expiry_days.clamp(0, 36500);
    policy.history_count = policy.history_count.clamp(0, 24);

    let items = [
        ("min_length", policy.min_length.to_string()),
        ("require_upper", policy.require_upper.to_string()),
        ("require_lower", policy.require_lower.to_string()),
        ("require_digit", policy.require_digit.to_string()),
        ("require_special", policy.require_special.to_string()),
        ("expiry_days", policy.expiry_days.to_string()),
        ("history_count", policy.history_count.to_string()),
    ];

    let mut tx = pool.begin().await?;
    for (key, value) in items {
        sqlx::query(
            "INSERT INTO system_configs (config_type, key, value)
             VALUES ('password_policy', $1, $2)
             ON CONFLICT (config_type, key) DO UPDATE SET value = $2, updated_at = NOW()",
        )
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// 按策略校验密码复杂度
pub fn check_complexity(policy: &PasswordPolicy, password: &str) -> Result<(), AppError> {
    if (password.len() as i32) < policy.min_length {
        return Err(AppError::Validation(
            msg("server.auth.password_too_short").with("min_length", policy.min_length),
        ));
    }

    let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_special = password
        .chars()
        .any(|c| c.is_ascii_graphic() && !c.is_alphanumeric());

    let mut missing = Vec::new();
    if policy.require_upper && !has_upper {
        missing.push("upper");
    }
    if policy.require_lower && !has_lower {
        missing.push("lower");
    }
    if policy.require_digit && !has_digit {
        missing.push("digit");
    }
    if policy.require_special && !has_special {
        missing.push("special");
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(
            msg("server.auth.password_complexity").with("missing", missing.join(",")),
        ))
    }
}

/// 校验密码复杂度（加载当前策略）
pub async fn validate_complexity(pool: &PgPool, password: &str) -> Result<(), AppError> {
    let policy = load(pool).await;
    check_complexity(&policy, password)
}

/// 校验新密码：复杂度 + 与历史密码不重复
pub async fn validate_password(
    pool: &PgPool,
    user_id: Uuid,
    password: &str,
) -> Result<(), AppError> {
    let policy = load(pool).await;
    check_complexity(&policy, password)?;

    if policy.history_count > 0 {
        // 当前密码始终参与比对，另取最近 N 条历史（users.created_at 早于
        // 全部历史记录，直接 UNION 排序会把当前密码挤出 LIMIT 窗口）
        let mut hashes: Vec<String> =
            sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(pool)
                .await?
                .flatten()
                .into_iter()
                .collect();
        let history: Vec<String> = sqlx::query_scalar(
            "SELECT password_hash FROM password_history WHERE user_id = $1
             ORDER BY created_at DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(policy.history_count)
        .fetch_all(pool)
        .await?;
        hashes.extend(history);

        for hash in hashes {
            if bcrypt::verify(password, &hash).unwrap_or(false) {
                return Err(AppError::Validation(msg("server.auth.password_reused")));
            }
        }
    }
    Ok(())
}

/// 记录密码历史（保留最近 history_count*2 条，防表膨胀）
pub async fn record_history(pool: &PgPool, user_id: Uuid, password_hash: &str) {
    let policy = load(pool).await;
    let keep = (policy.history_count.max(1) as i64) * 2;

    if let Err(e) =
        sqlx::query("INSERT INTO password_history (user_id, password_hash) VALUES ($1, $2)")
            .bind(user_id)
            .bind(password_hash)
            .execute(pool)
            .await
    {
        log_warn!("log.user.password_history_write_failed", error = e);
        return;
    }

    if let Err(e) = sqlx::query(
        "DELETE FROM password_history WHERE user_id = $1 AND id NOT IN (
            SELECT id FROM password_history WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2
        )",
    )
    .bind(user_id)
    .bind(keep)
    .execute(pool)
    .await
    {
        log_warn!("log.user.password_history_prune_failed", error = e);
    }
}

/// 密码是否已过有效期（策略关闭或用户无记录时返回 false）
pub async fn is_expired(pool: &PgPool, user_id: Uuid) -> Result<bool, AppError> {
    let policy = load(pool).await;
    if policy.expiry_days <= 0 {
        return Ok(false);
    }
    let changed_at: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT password_changed_at FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await?
            .flatten();
    let Some(changed_at) = changed_at else {
        return Ok(false);
    };
    Ok(Utc::now() > changed_at + chrono::Duration::days(policy.expiry_days as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_policy() -> PasswordPolicy {
        PasswordPolicy::default()
    }

    #[test]
    fn 复杂度_默认策略合法与非法() {
        assert!(check_complexity(&default_policy(), "Abcdef12").is_ok());
        assert!(check_complexity(&default_policy(), "Abcdef12345!@").is_ok());

        // 过短
        let err = check_complexity(&default_policy(), "Ab1").unwrap_err();
        let AppError::Validation(m) = err else {
            panic!("应为校验错误");
        };
        assert_eq!(m.key(), "server.auth.password_too_short");

        // 缺少大写与数字
        let err = check_complexity(&default_policy(), "onlylowercase").unwrap_err();
        let AppError::Validation(m) = err else {
            panic!("应为校验错误");
        };
        assert_eq!(m.key(), "server.auth.password_complexity");
    }

    #[test]
    fn 复杂度_特殊字符要求() {
        let mut policy = default_policy();
        policy.require_special = true;
        assert!(check_complexity(&policy, "Abcdef12!").is_ok());
        assert!(check_complexity(&policy, "Abcdef12").is_err());
    }

    #[test]
    fn 长度下限保护() {
        let mut policy = default_policy();
        policy.min_length = 4;
        assert_eq!(policy.min_length, 4);
        // 保存路径会钳制；直接调用 check 时以传入值为准
        assert!(check_complexity(&policy, "Ab12").is_ok());
    }
}
