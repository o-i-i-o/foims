CREATE OR REPLACE FUNCTION public.trg_encrypt_switches_passwords()
RETURNS trigger
LANGUAGE plpgsql
AS $function$
BEGIN
  -- 只在值发生变化时才加密
  IF NEW.snmp_community IS NOT NULL AND (OLD IS NULL OR NEW.snmp_community != OLD.snmp_community) THEN
    -- 检查是否已经是加密后的值（加密后的值通常以base64格式存储，长度较长）
    IF length(NEW.snmp_community) < 50 OR NEW.snmp_community NOT SIMILAR TO '[A-Za-z0-9+/=]+' THEN
      NEW.snmp_community := encrypt_password(NEW.snmp_community);
    END IF;
  END IF;
  IF NEW.snmp_auth_password IS NOT NULL AND (OLD IS NULL OR NEW.snmp_auth_password != OLD.snmp_auth_password) THEN
    IF length(NEW.snmp_auth_password) < 50 OR NEW.snmp_auth_password NOT SIMILAR TO '[A-Za-z0-9+/=]+' THEN
      NEW.snmp_auth_password := encrypt_password(NEW.snmp_auth_password);
    END IF;
  END IF;
  IF NEW.snmp_priv_password IS NOT NULL AND (OLD IS NULL OR NEW.snmp_priv_password != OLD.snmp_priv_password) THEN
    IF length(NEW.snmp_priv_password) < 50 OR NEW.snmp_priv_password NOT SIMILAR TO '[A-Za-z0-9+/=]+' THEN
      NEW.snmp_priv_password := encrypt_password(NEW.snmp_priv_password);
    END IF;
  END IF;
  NEW.updated_at := NOW();
  RETURN NEW;
END;
$function$;
