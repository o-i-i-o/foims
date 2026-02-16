-- 删除旧的触发器
DROP TRIGGER IF EXISTS trg_encrypt_switches_passwords_insert ON switches;
DROP TRIGGER IF EXISTS trg_encrypt_switches_passwords_update ON switches;

-- 重新创建函数
CREATE OR REPLACE FUNCTION public.trg_encrypt_switches_passwords()
RETURNS trigger
LANGUAGE plpgsql
AS $function$
BEGIN
  -- 只在值发生变化时才加密
  IF NEW.snmp_community IS NOT NULL AND (OLD IS NULL OR NEW.snmp_community != OLD.snmp_community) THEN
    -- 检查是否已经是加密后的值（加密后的值通常长度>=44且是base64格式）
    IF length(NEW.snmp_community) < 44 THEN
      NEW.snmp_community := encrypt_password(NEW.snmp_community);
    END IF;
  END IF;
  IF NEW.snmp_auth_password IS NOT NULL AND (OLD IS NULL OR NEW.snmp_auth_password != OLD.snmp_auth_password) THEN
    IF length(NEW.snmp_auth_password) < 44 THEN
      NEW.snmp_auth_password := encrypt_password(NEW.snmp_auth_password);
    END IF;
  END IF;
  IF NEW.snmp_priv_password IS NOT NULL AND (OLD IS NULL OR NEW.snmp_priv_password != OLD.snmp_priv_password) THEN
    IF length(NEW.snmp_priv_password) < 44 THEN
      NEW.snmp_priv_password := encrypt_password(NEW.snmp_priv_password);
    END IF;
  END IF;
  NEW.updated_at := NOW();
  RETURN NEW;
END;
$function$;

-- 重新创建触发器
CREATE TRIGGER trg_encrypt_switches_passwords_insert
BEFORE INSERT ON switches
FOR EACH ROW
EXECUTE FUNCTION public.trg_encrypt_switches_passwords();

CREATE TRIGGER trg_encrypt_switches_passwords_update
BEFORE UPDATE ON switches
FOR EACH ROW
EXECUTE FUNCTION public.trg_encrypt_switches_passwords();
