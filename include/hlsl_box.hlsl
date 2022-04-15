struct Input {
    float4 position: SV_POSITION;
    float2 coord: TEXCOORD0;
};

struct Parameters {
    float2 resolution;
    float2 mouse;
    float time;
};

ConstantBuffer<Parameters> HLSLBox: register(b0);

float2 normalized_position(float2 coord) {
    return (coord * 2.0 - HLSLBox.resolution) / min(HLSLBox.resolution.x, HLSLBox.resolution.y);
}