#!/bin/sh

# For interactive use of the container add:
#   --interactive
#   --tty
# or -it for the short form.    

# To run a chat client use:
#   docker exec -it vllm vllm chat

docker run \
       --rm \
       --device=/dev/kfd \
       --device=/dev/dri \
       --group-add video \
       --shm-size 16G \
       --security-opt seccomp=unconfined \
       --security-opt apparmor=unconfined \
       --cap-add=SYS_PTRACE \
       -v "${HOME}":/workspace \
       --env HUGGINGFACE_HUB_CACHE=/workspace \
       --network=host \
       --ipc=host \
       --name vllm \
       rocm/vllm:latest \
       vllm serve Qwen/Qwen3-30B-A3B-Instruct-2507 --max-model-len 262144 --disable-log-requests
