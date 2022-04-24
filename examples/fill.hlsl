#include "hlsl_box.hlsli"

float4 main(Input input): SV_Target {
    const float2 coord = input.coord / HLSLBox.resolution;
    return float4(coord.x, coord.y, 0.0, 1.0);
}