FROM centos:centos7.9.2009

RUN rm -f /etc/yum.repos.d/CentOS-*.repo
COPY centos7.repo /etc/yum.repos.d/CentOS-Vault.repo
RUN yum -y install git && yum clean all && git --version

WORKDIR /tmp
