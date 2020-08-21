FROM buildpack-deps AS base

RUN apt-get update && apt-get install -y build-essential cmake
WORKDIR /
RUN mkdir app
COPY . /app


FROM base AS tcc

WORKDIR /app
RUN cd vendor/tinycc && ./configure && make && make install

FROM tcc as birnam0
WORKDIR /app
COPY . /app
RUN make build
