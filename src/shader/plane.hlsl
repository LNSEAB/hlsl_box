#include "hlsl_box.hlsli"

struct VSInput {
    float3 position: POSITION;
    float2 coord: TEXCOORD0;
};

Input main(VSInput input) {
    Input output;
    output.position = float4(input.position, 1.0);
    output.coord = input.coord * HLSLBox.resolution;
    return output;
}
