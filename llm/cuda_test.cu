#include <stdio.h>
#include <cuda_runtime.h>

int main() {
    int count = 0;

    cudaError_t err = cudaGetDeviceCount(&count);

    if (err != cudaSuccess) {
        printf("CUDA error: %s\n", cudaGetErrorString(err));
        return 1;
    }

    printf("CUDA devices: %d\n", count);

    return 0;
}