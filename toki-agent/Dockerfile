FROM docker.io/cloudflare/sandbox:0.9.2-opencode

LABEL org.opencontainers.image.title="toki-agent-sandbox"
LABEL org.opencontainers.image.revision="opencode-1.14.33"

RUN apt-get update && apt-get install -y --no-install-recommends \
  build-essential \
  pkg-config \
  libssl-dev \
  libicu-dev \
  && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal
RUN curl -fsSL https://dot.net/v1/dotnet-install.sh -o /tmp/dotnet-install.sh \
  && bash /tmp/dotnet-install.sh --channel 10.0 --install-dir /usr/share/dotnet \
  && ln -s /usr/share/dotnet/dotnet /usr/bin/dotnet \
  && rm /tmp/dotnet-install.sh
RUN curl -fsSL https://opencode.ai/install | bash \
  && ln -sf /root/.opencode/bin/opencode /usr/local/bin/opencode \
  && ln -sf /root/.opencode/bin/opencode /usr/bin/opencode \
  && /root/.opencode/bin/opencode --version
ENV PATH="/root/.opencode/bin:/root/.cargo/bin:${PATH}"
