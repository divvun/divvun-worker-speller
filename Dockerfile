FROM debian:bookworm-slim

# Install required packages
RUN dpkg --add-architecture amd64 && \
    apt-get update && apt-get install -y \
    wget \
    gnupg2 \
    ca-certificates \
    lsb-release \
    && rm -rf /var/lib/apt/lists/*
# Download and install the nightly repository script
RUN wget https://apertium.projectjj.com/apt/install-nightly.sh \
    && bash install-nightly.sh \
    && rm install-nightly.sh
RUN apt-get update && apt-get install -y divvun-gramcheck:amd64 hfst:amd64 && rm -rf /var/lib/apt/lists/*
RUN apt-get update && apt-get upgrade -y && rm -rf /var/lib/apt/lists/*
VOLUME data
