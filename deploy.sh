#!/bin/bash -e
set -o pipefail

OVN_REPO=${OVN_REPO:-"https://github.com/ovn-org/ovn.git"}
OVN_BRANCH=${OVN_BRANCH:-"main"}
OVS_REPO=${OVS_REPO:-"https://github.com/openvswitch/ovs.git"}
OVS_BRANCH=${OVS_BRANCH:-"main"}
LOG_PATH=${LOG_PATH:-"/var/log/ovn-ci"}
HOSTNAME=${HOSTNAME:-$(hostname)}
USE_SUBMODULE=${USE_SUBMODULE:-"yes"}

function install_dependencies() {
    echo "Installing dependencies..."

    dnf -y install podman git nginx mailx @virtualization seavgabios-bin \
                   guestfs-tools edk2-ovmf edk2-aarch64
}

function setup_workspace() {
    echo "Setting up workspace..."
    rm -rf workspace
    mkdir workspace
}

function clone_repo() {
    name=$1
    repo=$2
    branch=$3

    echo "Cloning repo: $name (branch: $branch)..."

    git clone $repo workspace/$name --branch $branch --single-branch --depth 1
}

function init_submodule() {
    echo "Initializing OvS submodule in OVN..."
    cd workspace/ovn
    git submodule update --init --single-branch --depth 1
    cd -
}

function setup_nginx() {
    echo "Setting up nginx..."

    sed -e "s|@HOSTNAME@|$HOSTNAME|" -e "s|@LOG_PATH@|$LOG_PATH|" static/nginx.conf > /etc/nginx/nginx.conf
}

function compile_ovn_ci() {
    echo "Compiling ovn-ci..."

    podman pull docker.io/library/rust
    podman run --privileged \
    --rm \
    --user "$(id -u)":"$(id -g)" \
    -v $PWD:/usr/src/ovn-ci \
    -w /usr/src/ovn-ci \
    rust \
    cargo build --release
    install -m 755 target/release/ovn-ci /usr/bin/ovn-ci
}

function install_services() {
    echo "Installing services..."

    install -m 644 -D -t /usr/lib/systemd/system static/systemd/*
}

function create_directories() {
    echo "Creating directories..."

    mkdir -p $LOG_PATH
    mkdir -p /etc/ovn-ci
    mkdir -p /var/lib/ovn-ci
}


function setup_selinux() {
    checkmodule -M -m -o /tmp/virtlogd.mod static/selinux/virtlogd.te
    semodule_package -o /tmp/virtlogd.pp -m /tmp/virtlogd.mod
    semodule -i /tmp/virtlogd.pp

    semanage fcontext -a -t httpd_sys_content_t "$LOG_PATH(/.*)?" || true
    restorecon -R $LOG_PATH

    rm -f /tmp/virtlogd.mod /tmp/virtlogd.pp
}

function setup_firewalld() {
    firewall-cmd --permanent --add-port=8080/tcp
    firewall-cmd --reload
}

function create_ssh_key() {
    if [ ! -s /etc/ovn-ci/id_ed25519 ]; then
      ssh-keygen -t ed25519 -f /etc/ovn-ci/id_ed25519 -N ""
    fi
}

function configure_modular_libvirt() {
    echo "Configuring libvirt as modular..."

    systemctl stop libvirtd.service
    systemctl stop libvirtd{,-ro,-admin,-tcp,-tls}.socket

    systemctl disable libvirtd.service
    systemctl disable libvirtd{,-ro,-admin,-tcp,-tls}.socket

    for drv in qemu interface network nodedev nwfilter secret storage; do
      systemctl enable virt${drv}d.service
      systemctl enable virt${drv}d{,-ro,-admin}.socket
      systemctl start virt${drv}d{,-ro,-admin}.socket
    done
}

function start_services() {
    echo "Starting services..."

    for service in nginx.service ovn-ci.timer; do
        systemctl enable $service
        systemctl start $service
    done
}

function define_vm_network() {
    echo "Creating isolated libvirt network..."

    virsh net-destroy ovn-ci-isolated || true
    virsh net-undefine ovn-ci-isolated || true
    virsh net-define vm/network.xml
    virsh net-start ovn-ci-isolated
    virsh net-autostart ovn-ci-isolated
}

install_dependencies
setup_workspace
clone_repo ovn $OVN_REPO $OVN_BRANCH
if [ "$USE_SUBMODULE" = "yes" ]; then
    init_submodule
else
    clone_repo ovs $OVS_REPO $OVS_BRANCH
fi
setup_nginx
compile_ovn_ci
install_services
setup_firewalld
create_directories
setup_selinux
create_ssh_key
configure_modular_libvirt
start_services
define_vm_network
