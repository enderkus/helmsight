FROM rockylinux:9
RUN dnf -y install systemd procps-ng iproute openssh-server openssh-clients cronie nginx dnf-utils sudo \
 && dnf -y install epel-release && dnf -y install sshpass \
 && dnf makecache && ssh-keygen -A \
 && useradd -m -s /bin/sh monitor && mkdir -p /home/monitor/.ssh \
 && chown -R monitor:monitor /home/monitor/.ssh && chmod 700 /home/monitor/.ssh \
 && systemctl enable nginx sshd
COPY broken.service /etc/systemd/system/broken.service
CMD ["/sbin/init"]
