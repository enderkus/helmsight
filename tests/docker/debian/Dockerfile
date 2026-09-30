FROM debian:12
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    systemd systemd-sysv procps iproute2 openssh-server openssh-client sshpass cron nginx-light sudo \
 && rm -f /etc/ssh/ssh_host_*_key* && ssh-keygen -A \
 && useradd -m -s /bin/sh monitor && mkdir -p /home/monitor/.ssh /run/sshd \
 && chown -R monitor:monitor /home/monitor/.ssh && chmod 700 /home/monitor/.ssh
COPY broken.service /etc/systemd/system/broken.service
CMD ["/sbin/init"]
