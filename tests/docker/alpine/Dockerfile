FROM alpine:3.20
RUN apk add --no-cache openssh-server openssh-client sshpass openrc busybox-openrc sudo \
 && ssh-keygen -A \
 && adduser -D -s /bin/sh monitor && passwd -u monitor 2>/dev/null; \
    sed -i 's/^monitor:!/monitor:*/' /etc/shadow \
 && mkdir -p /home/monitor/.ssh && chown -R monitor:monitor /home/monitor/.ssh && chmod 700 /home/monitor/.ssh \
 && mkdir -p /run/openrc && touch /run/openrc/softlevel
COPY entrypoint.sh /entrypoint.sh
CMD ["/bin/sh", "/entrypoint.sh"]
