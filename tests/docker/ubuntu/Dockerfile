FROM ubuntu:24.04
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    systemd systemd-sysv procps iproute2 openssh-server openssh-client sshpass cron sudo \
 && rm -rf /var/lib/apt/lists/* \
 && rm -f /etc/ssh/ssh_host_*_key* && ssh-keygen -A \
 && useradd -m -s /bin/sh monitor && mkdir -p /home/monitor/.ssh /run/sshd \
 && chown -R monitor:monitor /home/monitor/.ssh && chmod 700 /home/monitor/.ssh \
 && systemctl enable ssh
COPY broken.service /etc/systemd/system/broken.service
CMD ["/sbin/init"]
