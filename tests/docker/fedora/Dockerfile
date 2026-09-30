FROM fedora:41
RUN dnf -y install systemd procps-ng iproute openssh-server openssh-clients sshpass cronie sudo \
 && dnf makecache && ssh-keygen -A \
 && useradd -m -s /bin/sh monitor && mkdir -p /home/monitor/.ssh \
 && chown -R monitor:monitor /home/monitor/.ssh && chmod 700 /home/monitor/.ssh \
 && systemctl enable sshd
COPY broken.service /etc/systemd/system/broken.service
CMD ["/sbin/init"]
