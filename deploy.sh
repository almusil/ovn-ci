#!/bin/bash -e
set -o pipefail

OVN_REPO=${OVN_REPO:-"https://github.com/ovn-org/ovn.git"}
OVN_BRANCH=${OVN_BRANCH:-"main"}
OVS_REPO=${OVS_REPO:-"https://github.com/openvswitch/ovs.git"}
OVS_BRANCH=${OVS_BRANCH:-"master"}
LOG_PATH=${LOG_PATH:-"/var/log/ovn-ci"}
HOSTNAME=${HOSTNAME:-$(hostname)}
USE_SUBMODULE=${USE_SUBMODULE:-"yes"}

function install_dependencies() {
    echo "Installing dependencies..."

    dnf -y install podman git nginx mailx
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
    mkdir -p $LOG_PATH
    semanage fcontext -a -t httpd_sys_content_t "$LOG_PATH(/.*)?" || true
    restorecon -R $LOG_PATH
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

function start_services() {
    echo "Starting services..."

    for service in nginx.service ovn-ci.timer; do
        systemctl enable $service
        systemctl start $service
    done
}

function create_etc() {
    echo "Creating /etc/ovn-ci config dir..."

    mkdir -p /etc/ovn-ci
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
start_services
