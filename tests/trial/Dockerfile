# One machine of the sync trial: a Linux distro, its Syncthing, Claude Code, and a user to run them.
ARG BASE
FROM ${BASE}
# "package" installs the distro's Syncthing; a version such as v2.1.6 installs that official release.
ARG SYNCTHING=package
ARG SYNCTHING_SHA256=
ARG CLAUDE_CODE=2.1.311
COPY install.sh /tmp/install.sh
RUN sh /tmp/install.sh "$SYNCTHING" "$SYNCTHING_SHA256"
USER dev
WORKDIR /home/dev
RUN curl -fsSL https://claude.ai/install.sh | bash -s "$CLAUDE_CODE" && mkdir work
ENV PATH=/home/dev/.local/bin:$PATH
