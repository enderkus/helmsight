#!/bin/sh
# Minimal init for the test container: syslog to /var/log/messages and sshd.
syslogd -O /var/log/messages
exec /usr/sbin/sshd -D
