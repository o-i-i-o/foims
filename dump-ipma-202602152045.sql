--
-- PostgreSQL database cluster dump
--

-- Started on 2026-02-15 20:45:48

SET default_transaction_read_only = off;

SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;

--
-- Roles
--

CREATE ROLE postgres;
ALTER ROLE postgres WITH SUPERUSER INHERIT CREATEROLE CREATEDB LOGIN REPLICATION BYPASSRLS;

--
-- User Configurations
--








--
-- Databases
--

--
-- Database "template1" dump
--

\connect template1

--
-- PostgreSQL database dump
--

-- Dumped from database version 17.7 (Ubuntu 17.7-0ubuntu0.25.10.1)
-- Dumped by pg_dump version 17.0

-- Started on 2026-02-15 20:45:48

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

-- Completed on 2026-02-15 20:45:49

--
-- PostgreSQL database dump complete
--

--
-- Database "ipma" dump
--

--
-- PostgreSQL database dump
--

-- Dumped from database version 17.7 (Ubuntu 17.7-0ubuntu0.25.10.1)
-- Dumped by pg_dump version 17.0

-- Started on 2026-02-15 20:45:50

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- TOC entry 3828 (class 1262 OID 16388)
-- Name: ipma; Type: DATABASE; Schema: -; Owner: postgres
--

CREATE DATABASE ipma WITH TEMPLATE = template0 ENCODING = 'UTF8' LOCALE_PROVIDER = libc LOCALE = 'en_US.UTF-8';


ALTER DATABASE ipma OWNER TO postgres;

\connect ipma

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- TOC entry 3 (class 3079 OID 44767)
-- Name: pgcrypto; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;


--
-- TOC entry 3829 (class 0 OID 0)
-- Dependencies: 3
-- Name: EXTENSION pgcrypto; Type: COMMENT; Schema: -; Owner: 
--

COMMENT ON EXTENSION pgcrypto IS 'cryptographic functions';


--
-- TOC entry 2 (class 3079 OID 16389)
-- Name: uuid-ossp; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS "uuid-ossp" WITH SCHEMA public;


--
-- TOC entry 3830 (class 0 OID 0)
-- Dependencies: 2
-- Name: EXTENSION "uuid-ossp"; Type: COMMENT; Schema: -; Owner: 
--

COMMENT ON EXTENSION "uuid-ossp" IS 'generate universally unique identifiers (UUIDs)';


--
-- TOC entry 269 (class 1255 OID 44717)
-- Name: check_ip_conflict(); Type: FUNCTION; Schema: public; Owner: postgres
--

CREATE FUNCTION public.check_ip_conflict() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM ip_managers 
        WHERE network_id = NEW.network_id 
        AND ip_address = NEW.ip_address
        AND status != 'inactive'
        AND id != COALESCE(NEW.id, '00000000-0000-0000-0000-000000000000')
    ) THEN
        RAISE EXCEPTION 'IP地址已占用: %', NEW.ip_address;
    END IF;
    RETURN NEW;
END;
$$;


ALTER FUNCTION public.check_ip_conflict() OWNER TO postgres;

--
-- TOC entry 260 (class 1255 OID 44715)
-- Name: check_position_overlap(); Type: FUNCTION; Schema: public; Owner: postgres
--

CREATE FUNCTION public.check_position_overlap() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM positions 
        WHERE cabinet_id = NEW.cabinet_id 
        AND id != COALESCE(NEW.id, '00000000-0000-0000-0000-000000000000')
        AND NOT (NEW.end_u < start_u OR NEW.start_u > end_u)
    ) THEN
        RAISE EXCEPTION '机柜位置U位重叠';
    END IF;
    RETURN NEW;
END;
$$;


ALTER FUNCTION public.check_position_overlap() OWNER TO postgres;

--
-- TOC entry 309 (class 1255 OID 44765)
-- Name: check_switch_circular_dependency(); Type: FUNCTION; Schema: public; Owner: postgres
--

CREATE FUNCTION public.check_switch_circular_dependency() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    current_id UUID;
    parent_id UUID;
    visited_ids UUID[] := '{}';
BEGIN
    -- 如果parent_switch_id为空，直接返回
    IF NEW.parent_switch_id IS NULL THEN
        RETURN NEW;
    END IF;
    
    -- 检查parent_switch_id是否等于当前记录的id，防止自引用
    IF NEW.parent_switch_id = NEW.id THEN
        RAISE EXCEPTION '交换机不能引用自身作为父交换机';
    END IF;
    
    -- 检查是否存在循环依赖
    current_id := NEW.parent_switch_id;
    
    WHILE current_id IS NOT NULL LOOP
        -- 检查是否已经访问过这个id（形成循环）
        IF current_id = ANY(visited_ids) THEN
            RAISE EXCEPTION '检测到循环依赖：交换机之间形成了循环引用';
        END IF;
        
        -- 检查是否回到了当前正在插入/更新的记录
        IF current_id = NEW.id THEN
            RAISE EXCEPTION '检测到循环依赖：交换机之间形成了循环引用';
        END IF;
        
        -- 将当前id添加到已访问列表
        visited_ids := visited_ids || current_id;
        
        -- 获取父交换机的parent_switch_id
        SELECT parent_switch_id INTO parent_id FROM switches WHERE id = current_id;
        
        -- 更新current_id为父交换机的parent_switch_id
        current_id := parent_id;
    END LOOP;
    
    RETURN NEW;
END;
$$;


ALTER FUNCTION public.check_switch_circular_dependency() OWNER TO postgres;

--
-- TOC entry 310 (class 1255 OID 44817)
-- Name: decrypt_password(text); Type: FUNCTION; Schema: public; Owner: postgres
--

CREATE FUNCTION public.decrypt_password(p_encrypted_password text) RETURNS text
    LANGUAGE plpgsql
    AS $$
DECLARE
  v_key TEXT;
  v_data BYTEA;
  v_iv BYTEA;
  v_ciphertext BYTEA;
BEGIN
  -- 获取加密密钥
  SELECT encryption_key INTO v_key FROM encryption_keys WHERE key_name = 'system_configs_key';
  
  -- 解码加密数据
  v_data := decode(p_encrypted_password, 'base64');
  
  -- 提取IV和密文
  v_iv := substring(v_data from 1 for 16);
  v_ciphertext := substring(v_data from 17);
  
  -- 使用AES-256-CBC解密
  RETURN convert_from(decrypt_iv(v_ciphertext, decode(v_key, 'base64'), v_iv, 'aes-cbc'), 'UTF8');
END;
$$;


ALTER FUNCTION public.decrypt_password(p_encrypted_password text) OWNER TO postgres;

--
-- TOC entry 297 (class 1255 OID 44816)
-- Name: encrypt_password(text); Type: FUNCTION; Schema: public; Owner: postgres
--

CREATE FUNCTION public.encrypt_password(p_password text) RETURNS text
    LANGUAGE plpgsql
    AS $$
DECLARE
  v_key TEXT;
  v_iv BYTEA;
BEGIN
  -- 获取加密密钥
  SELECT encryption_key INTO v_key FROM encryption_keys WHERE key_name = 'system_configs_key';
  
  -- 生成初始化向量
  v_iv := gen_random_bytes(16);
  
  -- 使用AES-256-CBC加密，将IV和密文一起存储
  RETURN encode(v_iv || encrypt_iv(p_password::bytea, decode(v_key, 'base64'), v_iv, 'aes-cbc'), 'base64');
END;
$$;


ALTER FUNCTION public.encrypt_password(p_password text) OWNER TO postgres;

--
-- TOC entry 311 (class 1255 OID 44824)
-- Name: trg_encrypt_switches_passwords(); Type: FUNCTION; Schema: public; Owner: postgres
--

CREATE FUNCTION public.trg_encrypt_switches_passwords() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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
$$;


ALTER FUNCTION public.trg_encrypt_switches_passwords() OWNER TO postgres;

--
-- TOC entry 296 (class 1255 OID 44821)
-- Name: trg_encrypt_system_configs_password(); Type: FUNCTION; Schema: public; Owner: postgres
--

CREATE FUNCTION public.trg_encrypt_system_configs_password() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  -- 加密smtp_pwd字段
  NEW.smtp_pwd := encrypt_password(NEW.smtp_pwd);
  NEW.updated_at := NOW();
  RETURN NEW;
END;
$$;


ALTER FUNCTION public.trg_encrypt_system_configs_password() OWNER TO postgres;

SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- TOC entry 246 (class 1259 OID 44892)
-- Name: cabinet_networks; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.cabinet_networks (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    room_id uuid,
    network_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    cabinet_id uuid NOT NULL
);


ALTER TABLE public.cabinet_networks OWNER TO postgres;

--
-- TOC entry 224 (class 1259 OID 44219)
-- Name: cabinets; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.cabinets (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    name character varying(50) NOT NULL,
    capacity integer DEFAULT 42 NOT NULL,
    description text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    room_id uuid NOT NULL
);


ALTER TABLE public.cabinets OWNER TO postgres;

--
-- TOC entry 3831 (class 0 OID 0)
-- Dependencies: 224
-- Name: TABLE cabinets; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.cabinets IS '机柜数据';


--
-- TOC entry 232 (class 1259 OID 44395)
-- Name: ip_managers; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.ip_managers (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    workstation_id uuid,
    position_id uuid,
    network_id uuid NOT NULL,
    ip_address inet NOT NULL,
    ip_version smallint DEFAULT 4 NOT NULL,
    mac_address character(17),
    hostname character varying(100),
    status character varying(20) DEFAULT 'active'::character varying NOT NULL,
    last_seen timestamp with time zone DEFAULT now() NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    switch_id uuid,
    device_type character varying(20),
    switch_port_id uuid,
    CONSTRAINT ip_version_check CHECK ((ip_version = ANY (ARRAY[4, 6])))
);


ALTER TABLE public.ip_managers OWNER TO postgres;

--
-- TOC entry 3832 (class 0 OID 0)
-- Dependencies: 232
-- Name: TABLE ip_managers; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.ip_managers IS 'IP管理';


--
-- TOC entry 221 (class 1259 OID 44144)
-- Name: network_cidrs; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.network_cidrs (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    name character varying(50) NOT NULL,
    network_region_id uuid NOT NULL,
    ipv4_cidr cidr,
    ipv6_cidr cidr,
    ipv4_gateway inet,
    ipv6_gateway inet,
    ipv4_dns inet,
    ipv6_dns inet,
    description text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    ip_usage_count integer DEFAULT 0,
    last_scan_at timestamp with time zone
);


ALTER TABLE public.network_cidrs OWNER TO postgres;

--
-- TOC entry 3833 (class 0 OID 0)
-- Dependencies: 221
-- Name: TABLE network_cidrs; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.network_cidrs IS '网段';


--
-- TOC entry 220 (class 1259 OID 44132)
-- Name: network_regions; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.network_regions (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    name character varying(20) NOT NULL,
    description text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.network_regions OWNER TO postgres;

--
-- TOC entry 3834 (class 0 OID 0)
-- Dependencies: 220
-- Name: TABLE network_regions; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.network_regions IS '网络区域';


--
-- TOC entry 226 (class 1259 OID 44270)
-- Name: positions; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.positions (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    name character varying(50) NOT NULL,
    cabinet_id uuid NOT NULL,
    start_u integer DEFAULT 1 NOT NULL,
    end_u integer DEFAULT 1 NOT NULL,
    description text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.positions OWNER TO postgres;

--
-- TOC entry 3835 (class 0 OID 0)
-- Dependencies: 226
-- Name: TABLE positions; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.positions IS '机位';


--
-- TOC entry 243 (class 1259 OID 44866)
-- Name: cabinet_position_ips; Type: VIEW; Schema: public; Owner: postgres
--

CREATE VIEW public.cabinet_position_ips AS
 SELECT imm.id,
    imm.position_id,
    concat(c.name, '-', cp.name) AS cabinet_position_name,
    cp.cabinet_id,
    c.name AS cabinet_name,
    imm.network_id,
    n.name AS network_name,
    nt.name AS network_region,
    imm.ip_address,
    imm.ip_version,
    imm.mac_address,
    imm.hostname,
    imm.status,
    imm.last_seen,
    imm.created_at,
    imm.updated_at
   FROM ((((public.ip_managers imm
     JOIN public.positions cp ON ((imm.position_id = cp.id)))
     JOIN public.cabinets c ON ((cp.cabinet_id = c.id)))
     JOIN public.network_cidrs n ON ((imm.network_id = n.id)))
     JOIN public.network_regions nt ON ((n.network_region_id = nt.id)))
  WHERE ((imm.device_type)::text = 'cabinet_position'::text);


ALTER VIEW public.cabinet_position_ips OWNER TO postgres;

--
-- TOC entry 241 (class 1259 OID 44804)
-- Name: encryption_keys; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.encryption_keys (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    key_name character varying(50) NOT NULL,
    encryption_key text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.encryption_keys OWNER TO postgres;

--
-- TOC entry 222 (class 1259 OID 44189)
-- Name: rooms; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.rooms (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    name character varying(50) NOT NULL,
    description text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    room_type character varying(20) DEFAULT 'OFFICE'::character varying,
    CONSTRAINT rooms_room_type_check CHECK (((room_type)::text = ANY (ARRAY[('OFFICE'::character varying)::text, ('DATA_CENTER'::character varying)::text])))
);


ALTER TABLE public.rooms OWNER TO postgres;

--
-- TOC entry 3836 (class 0 OID 0)
-- Dependencies: 222
-- Name: TABLE rooms; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.rooms IS '房间管理';


--
-- TOC entry 229 (class 1259 OID 44336)
-- Name: switch_ports; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.switch_ports (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    switch_id uuid NOT NULL,
    port_number character varying(30) NOT NULL,
    port_name character varying(50),
    port_type character varying(20) DEFAULT 'access'::character varying,
    vlan_id integer,
    status character varying(20) DEFAULT 'up'::character varying,
    speed character varying(20),
    description text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.switch_ports OWNER TO postgres;

--
-- TOC entry 3837 (class 0 OID 0)
-- Dependencies: 229
-- Name: TABLE switch_ports; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.switch_ports IS '交换机端口';


--
-- TOC entry 228 (class 1259 OID 44312)
-- Name: switches; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.switches (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    name character varying(100) NOT NULL,
    network_region_id uuid NOT NULL,
    ip_address inet NOT NULL,
    mac_address character(14),
    model character varying(100),
    vendor character varying(50),
    management_ip inet,
    location character varying(100),
    snmp_version character varying(3) DEFAULT 'v2c'::character varying,
    snmp_community character varying(64),
    snmp_username character varying(22),
    snmp_auth_protocol character varying(10),
    snmp_auth_password character varying(100),
    snmp_priv_protocol character varying(10),
    snmp_priv_password character varying(100),
    snmp_port integer DEFAULT 161,
    parent_switch_id uuid,
    parent_port_id uuid,
    description text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    network_id uuid NOT NULL
);


ALTER TABLE public.switches OWNER TO postgres;

--
-- TOC entry 3838 (class 0 OID 0)
-- Dependencies: 228
-- Name: TABLE switches; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.switches IS '交换机表';


--
-- TOC entry 225 (class 1259 OID 44250)
-- Name: workstations; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.workstations (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    name character varying(50) NOT NULL,
    room_id uuid NOT NULL,
    manager character varying(50),
    description text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.workstations OWNER TO postgres;

--
-- TOC entry 3839 (class 0 OID 0)
-- Dependencies: 225
-- Name: TABLE workstations; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.workstations IS '工位';


--
-- TOC entry 247 (class 1259 OID 44904)
-- Name: ip_managers_with_details; Type: VIEW; Schema: public; Owner: postgres
--

CREATE VIEW public.ip_managers_with_details AS
 SELECT imm.id,
    imm.workstation_id,
    imm.position_id,
    imm.switch_id,
    imm.switch_port_id,
    imm.device_type,
        CASE
            WHEN (w.id IS NOT NULL) THEN (w.name)::text
            WHEN (cp.id IS NOT NULL) THEN (cp.name)::text
            ELSE '未知设备'::text
        END AS device_name,
    imm.network_id,
        CASE
            WHEN (w.id IS NOT NULL) THEN (w.name)::text
            ELSE NULL::text
        END AS workstation_name,
        CASE
            WHEN (cp.id IS NOT NULL) THEN (cp.name)::text
            ELSE NULL::text
        END AS cabinet_position_name,
        CASE
            WHEN (s.id IS NOT NULL) THEN (s.name)::text
            ELSE NULL::text
        END AS switch_name,
    (sp.port_number)::text AS switch_port_number,
    (COALESCE(n.name, '未知'::character varying))::text AS network_name,
    (COALESCE(nt.name, '未知'::character varying))::text AS network_region,
    imm.ip_address,
    imm.ip_version,
    imm.mac_address,
    imm.hostname,
    imm.status,
    imm.last_seen,
    imm.created_at,
    imm.updated_at
   FROM ((((((((public.ip_managers imm
     LEFT JOIN public.workstations w ON ((imm.workstation_id = w.id)))
     LEFT JOIN public.rooms r ON ((w.room_id = r.id)))
     LEFT JOIN public.positions cp ON ((imm.position_id = cp.id)))
     LEFT JOIN public.cabinets c ON ((cp.cabinet_id = c.id)))
     LEFT JOIN public.switches s ON ((imm.switch_id = s.id)))
     LEFT JOIN public.switch_ports sp ON ((imm.switch_port_id = sp.id)))
     LEFT JOIN public.network_cidrs n ON ((imm.network_id = n.id)))
     LEFT JOIN public.network_regions nt ON ((n.network_region_id = nt.id)));


ALTER VIEW public.ip_managers_with_details OWNER TO postgres;

--
-- TOC entry 234 (class 1259 OID 44447)
-- Name: login_logs; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.login_logs (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    username character varying(50) NOT NULL,
    ip_address inet NOT NULL,
    user_agent character varying(255),
    success boolean NOT NULL,
    error_message character varying(255),
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.login_logs OWNER TO postgres;

--
-- TOC entry 3840 (class 0 OID 0)
-- Dependencies: 234
-- Name: TABLE login_logs; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.login_logs IS '登录日志';


--
-- TOC entry 238 (class 1259 OID 44719)
-- Name: network_usage; Type: VIEW; Schema: public; Owner: postgres
--

CREATE VIEW public.network_usage AS
SELECT
    NULL::uuid AS id,
    NULL::character varying(50) AS name,
    NULL::uuid AS network_region_id,
    NULL::cidr AS ipv4_cidr,
    NULL::cidr AS ipv6_cidr,
    NULL::inet AS ipv4_gateway,
    NULL::inet AS ipv6_gateway,
    NULL::inet AS ipv4_dns,
    NULL::inet AS ipv6_dns,
    NULL::text AS description,
    NULL::timestamp with time zone AS created_at,
    NULL::timestamp with time zone AS updated_at,
    NULL::integer AS ip_usage_count,
    NULL::timestamp with time zone AS last_scan_at,
    NULL::character varying(20) AS region_name,
    NULL::bigint AS used_ips,
    NULL::bigint AS total_ips;


ALTER VIEW public.network_usage OWNER TO postgres;

--
-- TOC entry 236 (class 1259 OID 44482)
-- Name: notifications; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.notifications (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    user_id uuid,
    title character varying(100) NOT NULL,
    content text NOT NULL,
    notification_type character varying(20) NOT NULL,
    read boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.notifications OWNER TO postgres;

--
-- TOC entry 3841 (class 0 OID 0)
-- Dependencies: 236
-- Name: TABLE notifications; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.notifications IS 'mac变动通知';


--
-- TOC entry 233 (class 1259 OID 44422)
-- Name: operation_logs; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.operation_logs (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    user_id uuid NOT NULL,
    action character varying(100) NOT NULL,
    resource_type character varying(50) NOT NULL,
    resource_id uuid NOT NULL,
    details jsonb DEFAULT '{}'::jsonb NOT NULL,
    result boolean NOT NULL,
    ip_address inet NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.operation_logs OWNER TO postgres;

--
-- TOC entry 3842 (class 0 OID 0)
-- Dependencies: 233
-- Name: TABLE operation_logs; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.operation_logs IS '操作日志表';


--
-- TOC entry 240 (class 1259 OID 44742)
-- Name: position_networks; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.position_networks (
    id uuid NOT NULL,
    position_id uuid,
    network_id uuid,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now()
);


ALTER TABLE public.position_networks OWNER TO postgres;

--
-- TOC entry 231 (class 1259 OID 44375)
-- Name: position_ports; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.position_ports (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    position_id uuid NOT NULL,
    switch_port_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.position_ports OWNER TO postgres;

--
-- TOC entry 3843 (class 0 OID 0)
-- Dependencies: 231
-- Name: TABLE position_ports; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.position_ports IS '机位端口';


--
-- TOC entry 235 (class 1259 OID 44456)
-- Name: revoked_tokens; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.revoked_tokens (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    token_hash character varying(255) NOT NULL,
    user_id uuid,
    revoked_at timestamp with time zone DEFAULT now() NOT NULL,
    expiry timestamp with time zone NOT NULL
);


ALTER TABLE public.revoked_tokens OWNER TO postgres;

--
-- TOC entry 3844 (class 0 OID 0)
-- Dependencies: 235
-- Name: TABLE revoked_tokens; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.revoked_tokens IS '登录令牌管理';


--
-- TOC entry 223 (class 1259 OID 44199)
-- Name: room_networks; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.room_networks (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    room_id uuid NOT NULL,
    network_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.room_networks OWNER TO postgres;

--
-- TOC entry 3845 (class 0 OID 0)
-- Dependencies: 223
-- Name: TABLE room_networks; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.room_networks IS '房间网段';


--
-- TOC entry 227 (class 1259 OID 44287)
-- Name: svg_layouts; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.svg_layouts (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    layout_type character varying(20) NOT NULL,
    room_id uuid,
    network_region_id uuid,
    element_id uuid NOT NULL,
    element_type character varying(20) NOT NULL,
    x integer DEFAULT 0 NOT NULL,
    y integer DEFAULT 0 NOT NULL,
    width integer DEFAULT 160 NOT NULL,
    height integer DEFAULT 160 NOT NULL,
    rotation integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.svg_layouts OWNER TO postgres;

--
-- TOC entry 3846 (class 0 OID 0)
-- Dependencies: 227
-- Name: TABLE svg_layouts; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.svg_layouts IS '机柜/工位布局布局表';


--
-- TOC entry 242 (class 1259 OID 44861)
-- Name: switch_ips; Type: VIEW; Schema: public; Owner: postgres
--

CREATE VIEW public.switch_ips AS
 SELECT imm.id,
    imm.switch_id,
    s.name AS switch_name,
    imm.network_id,
    n.name AS network_name,
    nt.name AS network_region,
    imm.ip_address,
    imm.ip_version,
    imm.mac_address,
    imm.hostname,
    imm.status,
    imm.last_seen,
    imm.created_at,
    imm.updated_at
   FROM (((public.ip_managers imm
     JOIN public.switches s ON ((imm.switch_id = s.id)))
     JOIN public.network_cidrs n ON ((imm.network_id = n.id)))
     JOIN public.network_regions nt ON ((n.network_region_id = nt.id)))
  WHERE ((imm.device_type)::text = 'switch'::text);


ALTER VIEW public.switch_ips OWNER TO postgres;

--
-- TOC entry 245 (class 1259 OID 44880)
-- Name: system_configs; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.system_configs (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    config_type character varying(50) NOT NULL,
    key character varying(100) NOT NULL,
    value text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.system_configs OWNER TO postgres;

--
-- TOC entry 237 (class 1259 OID 44519)
-- Name: system_configs_backup; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.system_configs_backup (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    smtp_ssl character varying(5) NOT NULL,
    smtp_user character varying(22) NOT NULL,
    smtp_pwd character varying(100) NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT system_configs_check CHECK (((smtp_ssl)::text = ANY ((ARRAY['true'::character varying, 'false'::character varying])::text[])))
);


ALTER TABLE public.system_configs_backup OWNER TO postgres;

--
-- TOC entry 3847 (class 0 OID 0)
-- Dependencies: 237
-- Name: TABLE system_configs_backup; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.system_configs_backup IS '系统配置表';


--
-- TOC entry 219 (class 1259 OID 44119)
-- Name: users; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.users (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    username character varying(50) NOT NULL,
    password_hash character varying(255) NOT NULL,
    email character varying(100) NOT NULL,
    role character varying(20) NOT NULL,
    status boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    reset_token character varying(255),
    reset_token_expiry timestamp without time zone,
    two_factor_secret character varying(255),
    two_factor_enabled boolean DEFAULT false,
    two_factor_verified boolean DEFAULT false
);


ALTER TABLE public.users OWNER TO postgres;

--
-- TOC entry 3848 (class 0 OID 0)
-- Dependencies: 219
-- Name: TABLE users; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.users IS '用户管理';


--
-- TOC entry 244 (class 1259 OID 44871)
-- Name: workstation_ips; Type: VIEW; Schema: public; Owner: postgres
--

CREATE VIEW public.workstation_ips AS
 SELECT imm.id,
    imm.workstation_id,
    concat(r.name, '-', w.name) AS workstation_name,
    w.room_id,
    r.name AS room_name,
    imm.network_id,
    n.name AS network_name,
    nt.name AS network_region,
    imm.ip_address,
    imm.ip_version,
    imm.mac_address,
    imm.hostname,
    imm.status,
    imm.last_seen,
    imm.created_at,
    imm.updated_at
   FROM ((((public.ip_managers imm
     JOIN public.workstations w ON ((imm.workstation_id = w.id)))
     JOIN public.rooms r ON ((w.room_id = r.id)))
     JOIN public.network_cidrs n ON ((imm.network_id = n.id)))
     JOIN public.network_regions nt ON ((n.network_region_id = nt.id)))
  WHERE ((imm.device_type)::text = 'workstation'::text);


ALTER VIEW public.workstation_ips OWNER TO postgres;

--
-- TOC entry 239 (class 1259 OID 44739)
-- Name: workstation_networks; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.workstation_networks (
    id uuid NOT NULL,
    network_id uuid,
    workstation_id uuid,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now()
);


ALTER TABLE public.workstation_networks OWNER TO postgres;

--
-- TOC entry 230 (class 1259 OID 44355)
-- Name: workstation_ports; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.workstation_ports (
    id uuid DEFAULT public.uuid_generate_v4() NOT NULL,
    workstation_id uuid NOT NULL,
    switch_port_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.workstation_ports OWNER TO postgres;

--
-- TOC entry 3849 (class 0 OID 0)
-- Dependencies: 230
-- Name: TABLE workstation_ports; Type: COMMENT; Schema: public; Owner: postgres
--

COMMENT ON TABLE public.workstation_ports IS '工位端口';


--
-- TOC entry 3822 (class 0 OID 44892)
-- Dependencies: 246
-- Data for Name: cabinet_networks; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.cabinet_networks (id, room_id, network_id, created_at, updated_at, cabinet_id) FROM stdin;
53a675bc-3785-49c0-9ce2-4c759c8a1730	\N	07740335-4ad4-4393-b515-db9d050bc5ed	2026-02-08 14:17:13.414966+00	2026-02-08 14:17:13.414966+00	a5ec8a06-3b34-49a1-9dea-4f726a7a1463
c5e8b7ec-77bc-4900-a42b-d8a920f23e54	\N	95233089-1fc4-451f-b9d2-b932eed33e21	2026-02-08 14:17:13.414966+00	2026-02-08 14:17:13.414966+00	a5ec8a06-3b34-49a1-9dea-4f726a7a1463
a89bd30c-1af9-4e5c-88cc-fea7c855e949	\N	95233089-1fc4-451f-b9d2-b932eed33e21	2026-02-08 14:19:09.253486+00	2026-02-08 14:19:09.253486+00	8291f866-7968-45ee-bdda-45f261206d69
440caf7b-f728-4856-9b1f-a3a9f2ed9bd9	\N	718d60e3-df98-428d-ba54-8cf5b2951a30	2026-02-09 11:52:24.583958+00	2026-02-09 11:52:24.583958+00	1db693f9-4c65-4696-ac37-ec0aa5b48a0a
\.


--
-- TOC entry 3804 (class 0 OID 44219)
-- Dependencies: 224
-- Data for Name: cabinets; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.cabinets (id, name, capacity, description, created_at, updated_at, room_id) FROM stdin;
1db693f9-4c65-4696-ac37-ec0aa5b48a0a	1#	42	\N	2026-02-09 11:52:24.583958+00	2026-02-09 11:52:24.583958+00	b3965c93-f19f-4040-8295-f92381d182fa
\.


--
-- TOC entry 3820 (class 0 OID 44804)
-- Dependencies: 241
-- Data for Name: encryption_keys; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.encryption_keys (id, key_name, encryption_key, created_at, updated_at) FROM stdin;
bb1006e7-78dd-4453-aa0f-6c174df3f155	system_configs_key	9XNnLLnvNay1U9H2tskQMCfX8GSSj/v4SZufjGs/8Ko=	2026-02-05 11:23:17.428583+00	2026-02-05 11:23:17.428583+00
\.


--
-- TOC entry 3812 (class 0 OID 44395)
-- Dependencies: 232
-- Data for Name: ip_managers; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.ip_managers (id, workstation_id, position_id, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at, switch_id, device_type, switch_port_id) FROM stdin;
c31d867b-e995-4da5-9222-daa6c9dce317	\N	\N	653b2d04-8f9a-4657-9d55-694f24bf47ea	172.16.254.66	4	\N	\N	active	2026-02-15 10:09:40.532187+00	2026-02-15 10:09:40.532187+00	2026-02-15 10:09:40.532187+00	2bf38a38-322d-40af-a637-04d6fd48e8f5	switch	\N
95659ca0-61eb-4841-95bf-85bdbcd20397	\N	\N	653b2d04-8f9a-4657-9d55-694f24bf47ea	172.16.254.65	4	\N	\N	active	2026-02-15 10:43:16.505631+00	2026-02-15 10:43:16.505631+00	2026-02-15 10:43:16.505631+00	7349517e-f570-4205-a6a8-6cd98eda4355	switch	\N
9a893861-a4d0-4103-b30e-e191f41da81b	\N	\N	653b2d04-8f9a-4657-9d55-694f24bf47ea	172.16.254.67	4	\N	\N	active	2026-02-15 10:45:59.340988+00	2026-02-15 10:45:59.340988+00	2026-02-15 10:45:59.340988+00	e2f31e9b-71ed-452d-9ca5-4097b3ff4535	switch	\N
ba39b535-ff07-4dfe-a162-9e537695ee5c	56fecd5c-39d9-4797-ab6c-15a03f686e9a	\N	e2abe6d5-f562-4bc3-b785-fda7c04b1cde	192.168.2.18	4	\N	\N	active	2026-02-15 11:16:08.025968+00	2026-02-15 11:16:08.025968+00	2026-02-15 11:16:08.025968+00	7349517e-f570-4205-a6a8-6cd98eda4355	workstation	fda92eb0-17e0-481c-b92f-52f4a1398006
0efe34d2-1c4a-4193-bf03-36881a170cc6	\N	5e7a437f-de66-4522-ba67-719c85fc9453	718d60e3-df98-428d-ba54-8cf5b2951a30	192.168.254.66	4	\N	\N	active	2026-02-15 11:16:54.030476+00	2026-02-15 11:16:54.030476+00	2026-02-15 11:16:54.030476+00	7349517e-f570-4205-a6a8-6cd98eda4355	cabinet_position	fda92eb0-17e0-481c-b92f-52f4a1398006
\.


--
-- TOC entry 3814 (class 0 OID 44447)
-- Dependencies: 234
-- Data for Name: login_logs; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.login_logs (id, username, ip_address, user_agent, success, error_message, created_at) FROM stdin;
6d76d4b9-d58d-459a-94fb-1eea282bf1eb	admin	192.168.139.9	Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36	t	\N	2026-02-03 11:39:41.430202+00
95723e7a-a7ec-402a-a4a9-e3a247375013	admin	192.168.139.9	Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36	t	\N	2026-02-03 12:34:55.115581+00
92019552-c4af-4df2-b9a0-b5987cc47954	admin	192.168.139.9	Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36	t	\N	2026-02-03 15:23:27.116967+00
5120a0d4-eccd-4afe-a4fe-581a5a613bda	admin	192.168.139.9	Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36	t	\N	2026-02-03 15:23:52.467152+00
68ce6595-4297-4107-b45d-7af62191354c	admin	192.168.139.9	Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36	t	\N	2026-02-03 15:24:49.755159+00
\.


--
-- TOC entry 3801 (class 0 OID 44144)
-- Dependencies: 221
-- Data for Name: network_cidrs; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.network_cidrs (id, name, network_region_id, ipv4_cidr, ipv6_cidr, ipv4_gateway, ipv6_gateway, ipv4_dns, ipv6_dns, description, created_at, updated_at, ip_usage_count, last_scan_at) FROM stdin;
653b2d04-8f9a-4657-9d55-694f24bf47ea	交换机管理-zwww	929c7ccc-f4be-47eb-a441-cbfe05aba66b	172.16.254.64/26	\N	\N	\N	\N	\N	\N	2026-02-09 07:49:10.22018+00	2026-02-09 07:54:44.718516+00	0	\N
5934d9ea-612f-4519-b69c-b7e864d756bc	二楼-jz	023064d2-0ff9-47a1-9a01-bf6780f186f3	192.168.9.0/24	\N	\N	\N	\N	\N	\N	2026-02-09 07:41:02.136283+00	2026-02-12 13:37:08.048785+00	0	\N
718d60e3-df98-428d-ba54-8cf5b2951a30	交换机管理-hlw	023064d2-0ff9-47a1-9a01-bf6780f186f3	192.168.254.64/26	fd02::/64	\N	\N	\N	\N	\N	2026-02-09 07:47:40.291693+00	2026-02-12 14:12:19.54574+00	0	\N
ae8cb210-97e5-413b-a61e-6ed3ced7082b	一楼办公室	929c7ccc-f4be-47eb-a441-cbfe05aba66b	172.16.1.0/24	\N	\N	\N	\N	\N	\N	2026-02-09 07:41:50.070023+00	2026-02-13 03:22:52.164332+00	0	\N
e2abe6d5-f562-4bc3-b785-fda7c04b1cde	一楼办公区	023064d2-0ff9-47a1-9a01-bf6780f186f3	192.168.2.0/24	fd0a::/80	\N	\N	\N	\N	\N	2026-02-09 07:33:19.970959+00	2026-02-13 03:23:15.829521+00	0	\N
531f6643-ecb0-4ccf-9649-1ca94e39626d	二楼-jz	929c7ccc-f4be-47eb-a441-cbfe05aba66b	172.16.18.0/24	\N	\N	\N	\N	\N	\N	2026-02-09 07:42:04.999715+00	2026-02-13 08:34:38.225992+00	0	\N
27d11137-58b2-4eba-a7a9-06661ad28206	傻瓜交换机	023064d2-0ff9-47a1-9a01-bf6780f186f3	0.0.1.0/24	\N	\N	\N	\N	\N	\N	2026-02-15 06:13:27.232004+00	2026-02-15 06:13:27.232004+00	0	\N
09d5cf95-d10f-44a4-8dcf-dc5a5d46920f	傻瓜交换机	929c7ccc-f4be-47eb-a441-cbfe05aba66b	0.0.2.0/24	\N	\N	\N	\N	\N	\N	2026-02-15 06:13:53.790923+00	2026-02-15 06:13:53.790923+00	0	\N
\.


--
-- TOC entry 3800 (class 0 OID 44132)
-- Dependencies: 220
-- Data for Name: network_regions; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.network_regions (id, name, description, created_at, updated_at) FROM stdin;
023064d2-0ff9-47a1-9a01-bf6780f186f3	hlw	\N	2026-02-03 10:29:29.320426+00	2026-02-13 03:23:12.614696+00
929c7ccc-f4be-47eb-a441-cbfe05aba66b	zwww	\N	2026-02-03 10:29:21.672701+00	2026-02-15 04:32:32.285218+00
\.


--
-- TOC entry 3816 (class 0 OID 44482)
-- Dependencies: 236
-- Data for Name: notifications; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.notifications (id, user_id, title, content, notification_type, read, created_at) FROM stdin;
\.


--
-- TOC entry 3813 (class 0 OID 44422)
-- Dependencies: 233
-- Data for Name: operation_logs; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.operation_logs (id, user_id, action, resource_type, resource_id, details, result, ip_address, created_at) FROM stdin;
\.


--
-- TOC entry 3819 (class 0 OID 44742)
-- Dependencies: 240
-- Data for Name: position_networks; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.position_networks (id, position_id, network_id, created_at, updated_at) FROM stdin;
\.


--
-- TOC entry 3811 (class 0 OID 44375)
-- Dependencies: 231
-- Data for Name: position_ports; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.position_ports (id, position_id, switch_port_id, created_at, updated_at) FROM stdin;
\.


--
-- TOC entry 3806 (class 0 OID 44270)
-- Dependencies: 226
-- Data for Name: positions; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.positions (id, name, cabinet_id, start_u, end_u, description, created_at, updated_at) FROM stdin;
5e7a437f-de66-4522-ba67-719c85fc9453	444	1db693f9-4c65-4696-ac37-ec0aa5b48a0a	1	1	\N	2026-02-10 07:08:10.336062+00	2026-02-15 11:16:54.030476+00
\.


--
-- TOC entry 3815 (class 0 OID 44456)
-- Dependencies: 235
-- Data for Name: revoked_tokens; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.revoked_tokens (id, token_hash, user_id, revoked_at, expiry) FROM stdin;
7ef9a266-b795-4731-a471-adf18a4272eb	9db362ca2474e31a67521fd40458504521b5e821bfc0cb90526a99efb502a6f4	fd69b0ff-e38f-455b-9371-ebd86c97e6ee	2026-02-14 11:42:19.344921+00	2026-02-15 07:51:57+00
4c574931-4283-4063-b5e6-c278c3c84304	33370045c58e63e7b75bd8977109d68478a049e6a2a1be074946d64836db810d	fd69b0ff-e38f-455b-9371-ebd86c97e6ee	2026-02-15 03:08:28.503541+00	2026-02-22 03:08:28+00
3cb2e45e-aa7e-410e-a5a7-031aab1f9107	c6c2570c02d32850995d5c66e18735c2d6999c31d5a70bb7d39418f1f75ad169	fd69b0ff-e38f-455b-9371-ebd86c97e6ee	2026-02-15 03:12:07.319476+00	2026-02-22 03:12:07+00
5f4ff06e-6b0d-406b-af61-38f2135dd94d	402250daa6018aebda07ba78c051cd20cfb18390f724b953618321f544086d1e	fd69b0ff-e38f-455b-9371-ebd86c97e6ee	2026-02-15 03:18:12.351685+00	2026-02-22 03:18:12+00
a9c842c2-7665-46a1-bf65-6ca810b9ef96	2cfaa04e15f06e33550211db40343b305d8877df9c6caf96a1c305f9d5ae8e0b	fd69b0ff-e38f-455b-9371-ebd86c97e6ee	2026-02-15 03:19:22.552167+00	2026-02-22 03:19:22+00
666ad1c6-9863-4afd-8cc4-a655c994cd36	6e5ff371768bb561dda0709b04d85cfa5d8f6f4770b8e512b250983b264f0f23	fd69b0ff-e38f-455b-9371-ebd86c97e6ee	2026-02-15 03:27:40.777817+00	2026-02-22 03:27:40+00
efc32443-038e-4b4f-a072-2116199519c2	323a9f3144e63212f4cef0d54f01cf22d96bbcff4a2912df2a7b39e4b161fa5c	fd69b0ff-e38f-455b-9371-ebd86c97e6ee	2026-02-15 03:31:43.643411+00	2026-02-22 03:31:43+00
71b9ea02-6aa6-425a-99e1-71165d7f9412	932eee5820100efdf129f537888cd2ad6180b2a783667a85e9502f65768eee51	fd69b0ff-e38f-455b-9371-ebd86c97e6ee	2026-02-15 03:35:42.53455+00	2026-02-22 03:35:42+00
f7dbf3d6-3ea1-4f19-a6fe-088e95e086d1	d69c0cc40db15adca9e9a56c0de8730d0c5e38a4f7af9cc8ddcd7f40f873daef	fd69b0ff-e38f-455b-9371-ebd86c97e6ee	2026-02-15 03:39:27.432171+00	2026-02-22 03:39:27+00
a331303e-6f12-4b41-a6ae-8407cec6c2d1	59b76dfc8141b86fd5fafe07c3551872c6330311c7e3d38299fbc34ccb5efb40	fd69b0ff-e38f-455b-9371-ebd86c97e6ee	2026-02-15 03:45:48.097316+00	2026-02-22 03:45:47+00
75883ba2-c495-4704-b2fe-61e26f4b752f	f884cc98dd2de3f6adfb897a39f772f93b409ded994036b8f3ec9467ea988b47	fd69b0ff-e38f-455b-9371-ebd86c97e6ee	2026-02-15 03:46:50.616316+00	2026-02-22 03:46:50+00
66ebcd82-edd9-45d2-b0b6-b52160c43213	906e09319f64b1523d4325f94d649089fe2e11d4d1891a6ec9482497916362de	fd69b0ff-e38f-455b-9371-ebd86c97e6ee	2026-02-15 03:49:32.381193+00	2026-02-22 03:49:32+00
bfef4873-2ceb-4a59-949b-95faf5c48d51	67c5c5415474a9e4d9fdbfd58bf3610578574986c8d839e7cfb3b71611486ac4	fd69b0ff-e38f-455b-9371-ebd86c97e6ee	2026-02-15 03:50:31.901044+00	2026-02-22 03:50:31+00
274219e2-f43f-4316-b56f-fefd7f6e3d39	0084fda1c619ab250285e6a4c00b7c7bf3c50104451ddf2ebf96d6e1e7c270e5	fd69b0ff-e38f-455b-9371-ebd86c97e6ee	2026-02-15 11:38:06.82099+00	2026-02-22 11:38:06+00
\.


--
-- TOC entry 3803 (class 0 OID 44199)
-- Dependencies: 223
-- Data for Name: room_networks; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.room_networks (id, room_id, network_id, created_at, updated_at) FROM stdin;
7c1b4a91-3dc8-4198-800e-209f4ae6489c	f6825a67-054f-4a77-b99f-b9d10cf74998	e2abe6d5-f562-4bc3-b785-fda7c04b1cde	2026-02-09 07:51:50.928474+00	2026-02-09 07:51:50.928474+00
3a629098-a902-41a0-8cc3-aceb50aafcac	d4d5ae72-e8db-40d4-956f-33a5160e351b	5934d9ea-612f-4519-b69c-b7e864d756bc	2026-02-09 07:52:45.767828+00	2026-02-09 07:52:45.767828+00
6ee99548-dec0-49ea-9372-268e8b4770b9	b3965c93-f19f-4040-8295-f92381d182fa	718d60e3-df98-428d-ba54-8cf5b2951a30	2026-02-09 07:53:39.565983+00	2026-02-09 07:53:39.565983+00
1b97f69f-8ad8-44d2-b1f1-8bdeea9fb429	ab4a681c-3490-4521-914d-150d8a5eab47	653b2d04-8f9a-4657-9d55-694f24bf47ea	2026-02-09 07:54:03.83763+00	2026-02-09 07:54:03.83763+00
\.


--
-- TOC entry 3802 (class 0 OID 44189)
-- Dependencies: 222
-- Data for Name: rooms; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.rooms (id, name, description, created_at, updated_at, room_type) FROM stdin;
f6825a67-054f-4a77-b99f-b9d10cf74998	112	\N	2026-02-09 07:51:50.928474+00	2026-02-09 07:51:50.928474+00	OFFICE
d4d5ae72-e8db-40d4-956f-33a5160e351b	jz	\N	2026-02-09 07:52:45.767828+00	2026-02-09 07:52:45.767828+00	OFFICE
b3965c93-f19f-4040-8295-f92381d182fa	互联网机房	\N	2026-02-09 07:53:39.565983+00	2026-02-09 07:53:39.565983+00	DATA_CENTER
ab4a681c-3490-4521-914d-150d8a5eab47	zwww机房	\N	2026-02-09 07:54:03.83763+00	2026-02-09 07:54:03.83763+00	DATA_CENTER
\.


--
-- TOC entry 3807 (class 0 OID 44287)
-- Dependencies: 227
-- Data for Name: svg_layouts; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.svg_layouts (id, layout_type, room_id, network_region_id, element_id, element_type, x, y, width, height, rotation, created_at, updated_at) FROM stdin;
8e2165c0-7749-4a6e-8e68-33458d3bcd25	workstation	f6825a67-054f-4a77-b99f-b9d10cf74998	\N	00000000-0000-0000-0000-000000000001	door	50	100	40	80	0	2026-02-12 15:20:55.388309+00	2026-02-12 15:20:55.388309+00
d4189146-3117-4d3f-82db-bff45ae266d2	workstation	f6825a67-054f-4a77-b99f-b9d10cf74998	\N	56fecd5c-39d9-4797-ab6c-15a03f686e9a	workstation	150	100	160	160	0	2026-02-12 15:20:55.900306+00	2026-02-12 15:20:55.900306+00
e0212770-f52a-4827-afc3-c2deedc010c7	network_region	\N	929c7ccc-f4be-47eb-a441-cbfe05aba66b	3eba374e-4f3b-431b-9d3f-58a2fb36fdda	network_device	100	200	150	500	0	2026-02-03 10:53:39.303212+00	2026-02-14 11:51:45.315824+00
f32a3ba1-8bf2-4744-86a6-b94ec3a63cb6	network_region	\N	023064d2-0ff9-47a1-9a01-bf6780f186f3	1db693f9-4c65-4696-ac37-ec0aa5b48a0a	network_device	50	100	150	880	0	2026-02-12 15:21:08.937902+00	2026-02-14 11:53:16.996475+00
\.


--
-- TOC entry 3809 (class 0 OID 44336)
-- Dependencies: 229
-- Data for Name: switch_ports; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.switch_ports (id, switch_id, port_number, port_name, port_type, vlan_id, status, speed, description, created_at, updated_at) FROM stdin;
fda92eb0-17e0-481c-b92f-52f4a1398006	7349517e-f570-4205-a6a8-6cd98eda4355	G1/3/0/6	\N	access	\N	up	\N	\N	2026-02-15 06:40:35.767967+00	2026-02-15 06:40:35.767967+00
\.


--
-- TOC entry 3808 (class 0 OID 44312)
-- Dependencies: 228
-- Data for Name: switches; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.switches (id, name, network_region_id, ip_address, mac_address, model, vendor, management_ip, location, snmp_version, snmp_community, snmp_username, snmp_auth_protocol, snmp_auth_password, snmp_priv_protocol, snmp_priv_password, snmp_port, parent_switch_id, parent_port_id, description, created_at, updated_at, network_id) FROM stdin;
7349517e-f570-4205-a6a8-6cd98eda4355	核心交换机	929c7ccc-f4be-47eb-a441-cbfe05aba66b	172.16.254.65	\N	\N	\N	\N	\N	v2c	jT1YEkpYnT7uecUSij+z4vd4Nj5oZXv91NQpp1ZeoDI=	\N	\N	\N	\N	\N	161	\N	\N	\N	2026-02-15 06:39:58.732408+00	2026-02-15 10:43:16.502029+00	653b2d04-8f9a-4657-9d55-694f24bf47ea
\.


--
-- TOC entry 3821 (class 0 OID 44880)
-- Dependencies: 245
-- Data for Name: system_configs; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.system_configs (id, config_type, key, value, created_at, updated_at) FROM stdin;
27737793-0476-480c-9d2f-e85692359e80	smtp	host	smtp.qq.com	2026-02-08 10:32:55.55183+00	2026-02-13 06:49:54.158631+00
eb45d659-699d-4212-b1ab-bb3f835d2db4	smtp	port	465	2026-02-08 10:32:55.55183+00	2026-02-13 06:49:54.158631+00
7cc0d099-4334-44f0-993a-e34e487b0eca	smtp	username	573523447@qq.com	2026-02-08 10:32:55.55183+00	2026-02-13 06:49:54.158631+00
cba5f73d-0ee8-4c76-a568-c09351e0e58e	smtp	password	e48b7ed00db3237517d644538c217dd444ef119e9290bb2fab498fb3b0c77d3f	2026-02-08 10:32:55.55183+00	2026-02-13 06:49:54.158631+00
a98061f8-bb4f-42d2-aa5c-5b8e53833b71	smtp	from	boss@oi-io.cc	2026-02-08 10:32:55.55183+00	2026-02-13 06:49:54.158631+00
1d52e594-fede-4da9-ab5c-d58bc459d56e	smtp	secure	true	2026-02-08 10:32:55.55183+00	2026-02-13 06:49:54.158631+00
\.


--
-- TOC entry 3817 (class 0 OID 44519)
-- Dependencies: 237
-- Data for Name: system_configs_backup; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.system_configs_backup (id, smtp_ssl, smtp_user, smtp_pwd, created_at, updated_at) FROM stdin;
11111111-1111-1111-1111-111111111111	true	test@example.com	QjXrdBVMdXLpXoModT9glUzczFB17eT0G5q9Wspj3DFtiSbXtN7TOXtN5FN+yaS6	2026-02-05 11:24:42.488182+00	2026-02-05 11:24:42.488182+00
\.


--
-- TOC entry 3799 (class 0 OID 44119)
-- Dependencies: 219
-- Data for Name: users; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.users (id, username, password_hash, email, role, status, created_at, updated_at, reset_token, reset_token_expiry, two_factor_secret, two_factor_enabled, two_factor_verified) FROM stdin;
fd69b0ff-e38f-455b-9371-ebd86c97e6ee	admin	$2b$12$i3VksAqT0CKjyt/4gFAMquDaZUbIqkKKSQP5uhk4Lyd5oIABsWmlu	boss@oi-io.cc	admin	t	2026-02-03 10:29:00.268659+00	2026-02-11 01:01:01.764765+00	呙閒𢓋	2026-02-09 06:17:17.217307	xL6kR8GFy0LFq76YptfBdZhaTv4=	f	f
5059bf0e-ce7f-48ed-967d-63f0a9a1ff24	testuser	$2b$12$LgqSuLYE2cIiR7nZGO/kHuJhZlgP543d6i4LGnCSxgrw.POZ1GXCq	wwfu06@gmail.com	admin	t	2026-02-11 08:08:38.762025+00	2026-02-11 08:08:54.10636+00	\N	\N	O5TPYMSHV5JRLOS5FSMWFA7PKM3QBTRA	t	t
\.


--
-- TOC entry 3818 (class 0 OID 44739)
-- Dependencies: 239
-- Data for Name: workstation_networks; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.workstation_networks (id, network_id, workstation_id, created_at, updated_at) FROM stdin;
\.


--
-- TOC entry 3810 (class 0 OID 44355)
-- Dependencies: 230
-- Data for Name: workstation_ports; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.workstation_ports (id, workstation_id, switch_port_id, created_at, updated_at) FROM stdin;
\.


--
-- TOC entry 3805 (class 0 OID 44250)
-- Dependencies: 225
-- Data for Name: workstations; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.workstations (id, name, room_id, manager, description, created_at, updated_at) FROM stdin;
56fecd5c-39d9-4797-ab6c-15a03f686e9a	1	f6825a67-054f-4a77-b99f-b9d10cf74998	王二	\N	2026-02-10 10:41:18.450696+00	2026-02-15 11:16:08.025968+00
\.


--
-- TOC entry 3553 (class 2606 OID 44229)
-- Name: cabinets cabinets_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.cabinets
    ADD CONSTRAINT cabinets_pkey PRIMARY KEY (id);


--
-- TOC entry 3615 (class 2606 OID 44815)
-- Name: encryption_keys encryption_keys_key_name_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.encryption_keys
    ADD CONSTRAINT encryption_keys_key_name_key UNIQUE (key_name);


--
-- TOC entry 3617 (class 2606 OID 44813)
-- Name: encryption_keys encryption_keys_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.encryption_keys
    ADD CONSTRAINT encryption_keys_pkey PRIMARY KEY (id);


--
-- TOC entry 3588 (class 2606 OID 44406)
-- Name: ip_managers ip_managers_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.ip_managers
    ADD CONSTRAINT ip_managers_pkey PRIMARY KEY (id);


--
-- TOC entry 3596 (class 2606 OID 44455)
-- Name: login_logs login_logs_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.login_logs
    ADD CONSTRAINT login_logs_pkey PRIMARY KEY (id);


--
-- TOC entry 3540 (class 2606 OID 44143)
-- Name: network_regions network_regions_name_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.network_regions
    ADD CONSTRAINT network_regions_name_key UNIQUE (name);


--
-- TOC entry 3542 (class 2606 OID 44141)
-- Name: network_regions network_regions_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.network_regions
    ADD CONSTRAINT network_regions_pkey PRIMARY KEY (id);


--
-- TOC entry 3544 (class 2606 OID 44153)
-- Name: network_cidrs networks_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.network_cidrs
    ADD CONSTRAINT networks_pkey PRIMARY KEY (id);


--
-- TOC entry 3619 (class 2606 OID 44891)
-- Name: system_configs new_system_configs_config_type_key_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.system_configs
    ADD CONSTRAINT new_system_configs_config_type_key_key UNIQUE (config_type, key);


--
-- TOC entry 3621 (class 2606 OID 44889)
-- Name: system_configs new_system_configs_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.system_configs
    ADD CONSTRAINT new_system_configs_pkey PRIMARY KEY (id);


--
-- TOC entry 3605 (class 2606 OID 44491)
-- Name: notifications notifications_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_pkey PRIMARY KEY (id);


--
-- TOC entry 3592 (class 2606 OID 44431)
-- Name: operation_logs operation_logs_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.operation_logs
    ADD CONSTRAINT operation_logs_pkey PRIMARY KEY (id);


--
-- TOC entry 3613 (class 2606 OID 44748)
-- Name: position_networks position_networks_pk; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.position_networks
    ADD CONSTRAINT position_networks_pk PRIMARY KEY (id);


--
-- TOC entry 3581 (class 2606 OID 44382)
-- Name: position_ports position_ports_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.position_ports
    ADD CONSTRAINT position_ports_pkey PRIMARY KEY (id);


--
-- TOC entry 3583 (class 2606 OID 44384)
-- Name: position_ports position_ports_position_id_switch_port_id_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.position_ports
    ADD CONSTRAINT position_ports_position_id_switch_port_id_key UNIQUE (position_id, switch_port_id);


--
-- TOC entry 3557 (class 2606 OID 44281)
-- Name: positions positions_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.positions
    ADD CONSTRAINT positions_pkey PRIMARY KEY (id);


--
-- TOC entry 3601 (class 2606 OID 44462)
-- Name: revoked_tokens revoked_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.revoked_tokens
    ADD CONSTRAINT revoked_tokens_pkey PRIMARY KEY (id);


--
-- TOC entry 3549 (class 2606 OID 44206)
-- Name: room_networks room_networks_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.room_networks
    ADD CONSTRAINT room_networks_pkey PRIMARY KEY (id);


--
-- TOC entry 3551 (class 2606 OID 44208)
-- Name: room_networks room_networks_room_id_network_id_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.room_networks
    ADD CONSTRAINT room_networks_room_id_network_id_key UNIQUE (room_id, network_id);


--
-- TOC entry 3546 (class 2606 OID 44198)
-- Name: rooms rooms_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.rooms
    ADD CONSTRAINT rooms_pkey PRIMARY KEY (id);


--
-- TOC entry 3559 (class 2606 OID 44301)
-- Name: svg_layouts svg_layouts_layout_type_room_id_network_region_id_element_i_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.svg_layouts
    ADD CONSTRAINT svg_layouts_layout_type_room_id_network_region_id_element_i_key UNIQUE (layout_type, room_id, network_region_id, element_id);


--
-- TOC entry 3561 (class 2606 OID 44299)
-- Name: svg_layouts svg_layouts_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.svg_layouts
    ADD CONSTRAINT svg_layouts_pkey PRIMARY KEY (id);


--
-- TOC entry 3563 (class 2606 OID 44901)
-- Name: svg_layouts svg_layouts_unique_layout; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.svg_layouts
    ADD CONSTRAINT svg_layouts_unique_layout UNIQUE NULLS NOT DISTINCT (layout_type, room_id, network_region_id, element_id);


--
-- TOC entry 3573 (class 2606 OID 44347)
-- Name: switch_ports switch_ports_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.switch_ports
    ADD CONSTRAINT switch_ports_pkey PRIMARY KEY (id);


--
-- TOC entry 3575 (class 2606 OID 44349)
-- Name: switch_ports switch_ports_switch_id_port_number_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.switch_ports
    ADD CONSTRAINT switch_ports_switch_id_port_number_key UNIQUE (switch_id, port_number);


--
-- TOC entry 3568 (class 2606 OID 44634)
-- Name: switches switches_ip_address_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.switches
    ADD CONSTRAINT switches_ip_address_key UNIQUE (ip_address);


--
-- TOC entry 3570 (class 2606 OID 44323)
-- Name: switches switches_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.switches
    ADD CONSTRAINT switches_pkey PRIMARY KEY (id);


--
-- TOC entry 3607 (class 2606 OID 44700)
-- Name: system_configs_backup system_configs_config_type_key_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.system_configs_backup
    ADD CONSTRAINT system_configs_config_type_key_key UNIQUE (smtp_ssl, smtp_user);


--
-- TOC entry 3609 (class 2606 OID 44526)
-- Name: system_configs_backup system_configs_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.system_configs_backup
    ADD CONSTRAINT system_configs_pkey PRIMARY KEY (id);


--
-- TOC entry 3534 (class 2606 OID 44131)
-- Name: users users_email_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_email_key UNIQUE (email);


--
-- TOC entry 3536 (class 2606 OID 44127)
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- TOC entry 3538 (class 2606 OID 44129)
-- Name: users users_username_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_username_key UNIQUE (username);


--
-- TOC entry 3611 (class 2606 OID 44746)
-- Name: workstation_networks workstation_networks_pk; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.workstation_networks
    ADD CONSTRAINT workstation_networks_pk PRIMARY KEY (id);


--
-- TOC entry 3577 (class 2606 OID 44362)
-- Name: workstation_ports workstation_ports_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.workstation_ports
    ADD CONSTRAINT workstation_ports_pkey PRIMARY KEY (id);


--
-- TOC entry 3579 (class 2606 OID 44364)
-- Name: workstation_ports workstation_ports_workstation_id_switch_port_id_key; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.workstation_ports
    ADD CONSTRAINT workstation_ports_workstation_id_switch_port_id_key UNIQUE (workstation_id, switch_port_id);


--
-- TOC entry 3555 (class 2606 OID 44264)
-- Name: workstations workstations_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.workstations
    ADD CONSTRAINT workstations_pkey PRIMARY KEY (id);


--
-- TOC entry 3584 (class 1259 OID 44529)
-- Name: idx_ip_managers_ip_address; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_ip_managers_ip_address ON public.ip_managers USING btree (ip_address);


--
-- TOC entry 3585 (class 1259 OID 44830)
-- Name: idx_ip_managers_mac_address; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_ip_managers_mac_address ON public.ip_managers USING btree (mac_address);


--
-- TOC entry 3586 (class 1259 OID 44499)
-- Name: idx_ip_managers_workstation_id; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_ip_managers_workstation_id ON public.ip_managers USING btree (workstation_id);


--
-- TOC entry 3593 (class 1259 OID 44505)
-- Name: idx_login_logs_created_at; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_login_logs_created_at ON public.login_logs USING btree (created_at);


--
-- TOC entry 3594 (class 1259 OID 44504)
-- Name: idx_login_logs_username; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_login_logs_username ON public.login_logs USING btree (username);


--
-- TOC entry 3602 (class 1259 OID 44514)
-- Name: idx_notifications_created_at; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_notifications_created_at ON public.notifications USING btree (created_at);


--
-- TOC entry 3603 (class 1259 OID 44513)
-- Name: idx_notifications_user_id; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_notifications_user_id ON public.notifications USING btree (user_id);


--
-- TOC entry 3589 (class 1259 OID 44503)
-- Name: idx_operation_logs_created_at; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_operation_logs_created_at ON public.operation_logs USING btree (created_at);


--
-- TOC entry 3590 (class 1259 OID 44502)
-- Name: idx_operation_logs_user_id; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_operation_logs_user_id ON public.operation_logs USING btree (user_id);


--
-- TOC entry 3597 (class 1259 OID 44508)
-- Name: idx_revoked_tokens_expiry; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_revoked_tokens_expiry ON public.revoked_tokens USING btree (expiry);


--
-- TOC entry 3598 (class 1259 OID 44506)
-- Name: idx_revoked_tokens_token_hash; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_revoked_tokens_token_hash ON public.revoked_tokens USING btree (token_hash);


--
-- TOC entry 3599 (class 1259 OID 44507)
-- Name: idx_revoked_tokens_user_id; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_revoked_tokens_user_id ON public.revoked_tokens USING btree (user_id);


--
-- TOC entry 3547 (class 1259 OID 44713)
-- Name: idx_room_networks_room_id; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_room_networks_room_id ON public.room_networks USING btree (room_id);


--
-- TOC entry 3571 (class 1259 OID 44517)
-- Name: idx_switch_ports_switch_id; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_switch_ports_switch_id ON public.switch_ports USING btree (switch_id);


--
-- TOC entry 3564 (class 1259 OID 44635)
-- Name: idx_switches_ip_address; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_switches_ip_address ON public.switches USING btree (ip_address);


--
-- TOC entry 3565 (class 1259 OID 44711)
-- Name: idx_switches_network_region_id; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_switches_network_region_id ON public.switches USING btree (network_region_id);


--
-- TOC entry 3566 (class 1259 OID 44516)
-- Name: idx_switches_parent_switch_id; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_switches_parent_switch_id ON public.switches USING btree (parent_switch_id);


--
-- TOC entry 3531 (class 1259 OID 44498)
-- Name: idx_users_email; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_users_email ON public.users USING btree (email);


--
-- TOC entry 3532 (class 1259 OID 44497)
-- Name: idx_users_username; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX idx_users_username ON public.users USING btree (username);


--
-- TOC entry 3794 (class 2618 OID 44722)
-- Name: network_usage _RETURN; Type: RULE; Schema: public; Owner: postgres
--

CREATE OR REPLACE VIEW public.network_usage AS
 SELECT n.id,
    n.name,
    n.network_region_id,
    n.ipv4_cidr,
    n.ipv6_cidr,
    n.ipv4_gateway,
    n.ipv6_gateway,
    n.ipv4_dns,
    n.ipv6_dns,
    n.description,
    n.created_at,
    n.updated_at,
    n.ip_usage_count,
    n.last_scan_at,
    nr.name AS region_name,
    count(DISTINCT
        CASE
            WHEN ((ip.status)::text = 'active'::text) THEN ip.id
            ELSE NULL::uuid
        END) AS used_ips,
        CASE
            WHEN (n.ipv4_cidr IS NOT NULL) THEN (power((2)::double precision, ((32 - masklen((n.ipv4_cidr)::inet)))::double precision))::bigint
            WHEN (n.ipv6_cidr IS NOT NULL) THEN (power((2)::double precision, ((128 - masklen((n.ipv6_cidr)::inet)))::double precision))::bigint
            ELSE NULL::bigint
        END AS total_ips
   FROM ((public.network_cidrs n
     JOIN public.network_regions nr ON ((n.network_region_id = nr.id)))
     LEFT JOIN public.ip_managers ip ON ((n.id = ip.network_id)))
  GROUP BY n.id, nr.id;


--
-- TOC entry 3646 (class 2620 OID 44718)
-- Name: ip_managers trg_check_ip_conflict; Type: TRIGGER; Schema: public; Owner: postgres
--

CREATE TRIGGER trg_check_ip_conflict BEFORE INSERT OR UPDATE ON public.ip_managers FOR EACH ROW EXECUTE FUNCTION public.check_ip_conflict();


--
-- TOC entry 3642 (class 2620 OID 44716)
-- Name: positions trg_check_position_overlap; Type: TRIGGER; Schema: public; Owner: postgres
--

CREATE TRIGGER trg_check_position_overlap BEFORE INSERT OR UPDATE ON public.positions FOR EACH ROW EXECUTE FUNCTION public.check_position_overlap();


--
-- TOC entry 3643 (class 2620 OID 44766)
-- Name: switches trg_check_switch_circular_dependency; Type: TRIGGER; Schema: public; Owner: postgres
--

CREATE TRIGGER trg_check_switch_circular_dependency BEFORE INSERT OR UPDATE OF parent_switch_id ON public.switches FOR EACH ROW EXECUTE FUNCTION public.check_switch_circular_dependency();


--
-- TOC entry 3644 (class 2620 OID 44902)
-- Name: switches trg_encrypt_switches_passwords_insert; Type: TRIGGER; Schema: public; Owner: postgres
--

CREATE TRIGGER trg_encrypt_switches_passwords_insert BEFORE INSERT ON public.switches FOR EACH ROW EXECUTE FUNCTION public.trg_encrypt_switches_passwords();


--
-- TOC entry 3645 (class 2620 OID 44903)
-- Name: switches trg_encrypt_switches_passwords_update; Type: TRIGGER; Schema: public; Owner: postgres
--

CREATE TRIGGER trg_encrypt_switches_passwords_update BEFORE UPDATE ON public.switches FOR EACH ROW EXECUTE FUNCTION public.trg_encrypt_switches_passwords();


--
-- TOC entry 3647 (class 2620 OID 44822)
-- Name: system_configs_backup trg_encrypt_system_configs_password_insert; Type: TRIGGER; Schema: public; Owner: postgres
--

CREATE TRIGGER trg_encrypt_system_configs_password_insert BEFORE INSERT ON public.system_configs_backup FOR EACH ROW EXECUTE FUNCTION public.trg_encrypt_system_configs_password();


--
-- TOC entry 3648 (class 2620 OID 44823)
-- Name: system_configs_backup trg_encrypt_system_configs_password_update; Type: TRIGGER; Schema: public; Owner: postgres
--

CREATE TRIGGER trg_encrypt_system_configs_password_update BEFORE UPDATE OF smtp_pwd ON public.system_configs_backup FOR EACH ROW EXECUTE FUNCTION public.trg_encrypt_system_configs_password();


--
-- TOC entry 3636 (class 2606 OID 44417)
-- Name: ip_managers ip_managers_network_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.ip_managers
    ADD CONSTRAINT ip_managers_network_id_fkey FOREIGN KEY (network_id) REFERENCES public.network_cidrs(id);


--
-- TOC entry 3637 (class 2606 OID 44412)
-- Name: ip_managers ip_managers_position_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.ip_managers
    ADD CONSTRAINT ip_managers_position_id_fkey FOREIGN KEY (position_id) REFERENCES public.positions(id) ON DELETE SET NULL;


--
-- TOC entry 3638 (class 2606 OID 44407)
-- Name: ip_managers ip_managers_workstation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.ip_managers
    ADD CONSTRAINT ip_managers_workstation_id_fkey FOREIGN KEY (workstation_id) REFERENCES public.workstations(id) ON DELETE SET NULL;


--
-- TOC entry 3622 (class 2606 OID 44154)
-- Name: network_cidrs networks_network_region_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.network_cidrs
    ADD CONSTRAINT networks_network_region_id_fkey FOREIGN KEY (network_region_id) REFERENCES public.network_regions(id);


--
-- TOC entry 3641 (class 2606 OID 44492)
-- Name: notifications notifications_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- TOC entry 3639 (class 2606 OID 44432)
-- Name: operation_logs operation_logs_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.operation_logs
    ADD CONSTRAINT operation_logs_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- TOC entry 3634 (class 2606 OID 44385)
-- Name: position_ports position_ports_position_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.position_ports
    ADD CONSTRAINT position_ports_position_id_fkey FOREIGN KEY (position_id) REFERENCES public.positions(id) ON DELETE CASCADE;


--
-- TOC entry 3635 (class 2606 OID 44390)
-- Name: position_ports position_ports_switch_port_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.position_ports
    ADD CONSTRAINT position_ports_switch_port_id_fkey FOREIGN KEY (switch_port_id) REFERENCES public.switch_ports(id) ON DELETE CASCADE;


--
-- TOC entry 3626 (class 2606 OID 44282)
-- Name: positions positions_cabinet_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.positions
    ADD CONSTRAINT positions_cabinet_id_fkey FOREIGN KEY (cabinet_id) REFERENCES public.cabinets(id) ON DELETE CASCADE;


--
-- TOC entry 3640 (class 2606 OID 44463)
-- Name: revoked_tokens revoked_tokens_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.revoked_tokens
    ADD CONSTRAINT revoked_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- TOC entry 3623 (class 2606 OID 44214)
-- Name: room_networks room_networks_network_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.room_networks
    ADD CONSTRAINT room_networks_network_id_fkey FOREIGN KEY (network_id) REFERENCES public.network_cidrs(id);


--
-- TOC entry 3624 (class 2606 OID 44209)
-- Name: room_networks room_networks_room_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.room_networks
    ADD CONSTRAINT room_networks_room_id_fkey FOREIGN KEY (room_id) REFERENCES public.rooms(id) ON DELETE CASCADE;


--
-- TOC entry 3627 (class 2606 OID 44307)
-- Name: svg_layouts svg_layouts_network_region_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.svg_layouts
    ADD CONSTRAINT svg_layouts_network_region_id_fkey FOREIGN KEY (network_region_id) REFERENCES public.network_regions(id) ON DELETE CASCADE;


--
-- TOC entry 3628 (class 2606 OID 44302)
-- Name: svg_layouts svg_layouts_room_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.svg_layouts
    ADD CONSTRAINT svg_layouts_room_id_fkey FOREIGN KEY (room_id) REFERENCES public.rooms(id) ON DELETE CASCADE;


--
-- TOC entry 3631 (class 2606 OID 44350)
-- Name: switch_ports switch_ports_switch_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.switch_ports
    ADD CONSTRAINT switch_ports_switch_id_fkey FOREIGN KEY (switch_id) REFERENCES public.switches(id) ON DELETE CASCADE;


--
-- TOC entry 3629 (class 2606 OID 44326)
-- Name: switches switches_network_region_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.switches
    ADD CONSTRAINT switches_network_region_id_fkey FOREIGN KEY (network_region_id) REFERENCES public.network_regions(id);


--
-- TOC entry 3630 (class 2606 OID 44331)
-- Name: switches switches_parent_switch_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.switches
    ADD CONSTRAINT switches_parent_switch_id_fkey FOREIGN KEY (parent_switch_id) REFERENCES public.switches(id) ON DELETE SET NULL;


--
-- TOC entry 3632 (class 2606 OID 44370)
-- Name: workstation_ports workstation_ports_switch_port_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.workstation_ports
    ADD CONSTRAINT workstation_ports_switch_port_id_fkey FOREIGN KEY (switch_port_id) REFERENCES public.switch_ports(id) ON DELETE CASCADE;


--
-- TOC entry 3633 (class 2606 OID 44365)
-- Name: workstation_ports workstation_ports_workstation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.workstation_ports
    ADD CONSTRAINT workstation_ports_workstation_id_fkey FOREIGN KEY (workstation_id) REFERENCES public.workstations(id) ON DELETE CASCADE;


--
-- TOC entry 3625 (class 2606 OID 44265)
-- Name: workstations workstations_room_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.workstations
    ADD CONSTRAINT workstations_room_id_fkey FOREIGN KEY (room_id) REFERENCES public.rooms(id);


-- Completed on 2026-02-15 20:45:50

--
-- PostgreSQL database dump complete
--

-- Completed on 2026-02-15 20:45:50

--
-- PostgreSQL database cluster dump complete
--

