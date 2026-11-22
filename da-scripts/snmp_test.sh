#!/bin/sh
#用法：./snmp_test.sh -3 | -2 [-h 主机] [-o OID]
#-3：仅测 SNMPv3（SHA/AES-192 authPriv）；-2：仅测 SNMPv2c（community public）
#-h：目标主机（默认 localhost）；-o：OID（默认 1.3.6.1.2.1.1.1.0，即 sysDescr）
HOST=localhost
OID=1.3.6.1.2.1.1.1.0
FLAG=

while getopts "32h:o:" opt; do
    case "$opt" in
        3) FLAG=3 ;;
        2) FLAG=2 ;;
        h) HOST=$OPTARG ;;
        o) OID=$OPTARG ;;
        *)  echo "用法：$0 -3 | -2 [-h 主机] [-o OID]" >&2
            exit 1
            ;;
    esac
done

case "$FLAG" in
    3)
        exec snmpget -v3 -u test -l authPriv -a SHA -A "zxcvbnm," -x AES-192 -X "zxcvbnm," "$HOST" "$OID"
        ;;
    2)
        exec snmpget -v 2c -c public "$HOST" "$OID"
        ;;
    *)
        echo "用法：$0 -3 | -2 [-h 主机] [-o OID]" >&2
        exit 1
        ;;
esac
