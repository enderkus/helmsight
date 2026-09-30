FROM opensuse/leap:15.6
RUN zypper -n --gpg-auto-import-keys refresh \
 && zypper -n install systemd systemd-sysvinit procps iproute2 openssh-server openssh-clients cron sudo shadow \
 && ssh-keygen -A \
 && useradd -m -s /bin/sh monitor && mkdir -p /home/monitor/.ssh \
 && chown -R monitor /home/monitor/.ssh && chmod 700 /home/monitor/.ssh \
 && systemctl enable sshd
COPY broken.service /etc/systemd/system/broken.service
CMD ["/sbin/init"]
