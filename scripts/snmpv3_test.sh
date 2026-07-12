#!/bin/sh
snmpget -v3 -u test -l authPriv -a SHA -A "zxcvbnm," -x AES-192 -X "zxcvbnm," 192.168.254.65 1.3.6.1.2.1.1.1.0
