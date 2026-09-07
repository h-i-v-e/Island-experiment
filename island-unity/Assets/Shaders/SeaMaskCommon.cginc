#ifndef MOTU_SEA_MASK_INCLUDED
#define MOTU_SEA_MASK_INCLUDED

// RGBA8 encoding shared with island-rs/src/sea_mask.rs.
static const float MotuSeaMaskDepthMetres = 5.0;
static const float MotuSeaMaskLandDistanceMetres = 128.0;
// Ordinary swell and shallow overlay weighting retain their near-shore range.
static const float MotuOceanAttenuationDistanceMetres = 16.0;

#endif
