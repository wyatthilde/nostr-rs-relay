docker buildx build \                                                                             
  --platform linux/amd64,linux/arm64 \
  -t wyatthilde/nostr-relay:latest \
  --push .