#!/usr/bin/env bash
set -ex

[[ $(uname) = Linux ]] || exit 1
[[ $USER = root ]] || exit 1

sudo sysctl -w net.core.rmem_default=1610612736
sudo sysctl -w net.core.rmem_max=1610612736

sudo sysctl -w net.core.wmem_default=1610612736
sudo sysctl -w net.core.wmem_max=1610612736

echo "MaxAuthTries 60" | sudo tee -a /etc/ssh/sshd_config
sudo service sshd restart
sudo systemctl restart sshd
