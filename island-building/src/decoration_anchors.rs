use motu::Vec3;
use serde::{Deserialize, Serialize, Serializer};

/// A stable, labelled pose for mounting a decoration in native building-local space.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DecorationAnchor {
    /// Stable identifier within this building's decoration anchors.
    pub id: String,
    /// Runtime decoration label, which multiple anchors can share.
    pub label: String,
    /// Mounting point in metres with native +Z up.
    pub position: [f32; 3],
    /// Surface normal pointing away from the mounting plane.
    pub normal: [f32; 3],
    /// Right-handed rotation about the normal in degrees.
    pub twist_degrees: f32,
}

impl DecorationAnchor {
    pub(crate) fn unit_normal(&self) -> Option<Vec3> {
        let [x, y, z] = self.normal;
        // Scale before taking the length so finite inputs cannot overflow or underflow.
        let scale = x.abs().max(y.abs()).max(z.abs());
        Vec3::new(x / scale, y / scale, z / scale).try_normalize()
    }

    /// Unit tangent obtained by projecting +Z onto the mounting plane, then applying twist.
    /// Use +Y as the reference when the absolute dot product of the unit normal and +Z exceeds 0.95.
    #[must_use]
    pub fn forward(&self) -> [f32; 3] {
        self.forward_about(self.unit_normal().unwrap_or(Vec3::Z))
            .to_array()
    }

    fn forward_about(&self, normal: Vec3) -> Vec3 {
        let reference = if normal.z.abs() > 0.95 {
            Vec3::Y
        } else {
            Vec3::Z
        };
        let tangent = (reference - normal * normal.dot(reference)).normalize();
        let twist = if self.twist_degrees.is_finite() {
            self.twist_degrees % 360.
        } else {
            0.
        };
        let (sin, cos) = twist.to_radians().sin_cos();
        (tangent * cos + normal.cross(tangent) * sin).normalize()
    }
}

#[derive(Serialize)]
struct DecorationAnchorPose<'a> {
    id: &'a str,
    label: &'a str,
    position: [f32; 3],
    normal: [f32; 3],
    forward: [f32; 3],
}

/// The shared native and portable export shape for resolved mounting poses.
pub(crate) struct DecorationAnchorPoses<'a>(pub &'a [DecorationAnchor]);

impl Serialize for DecorationAnchorPoses<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().map(|anchor| {
            let normal = anchor.unit_normal().unwrap_or(Vec3::Z);
            DecorationAnchorPose {
                id: &anchor.id,
                label: &anchor.label,
                position: anchor.position,
                normal: normal.to_array(),
                forward: anchor.forward_about(normal).to_array(),
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuildingRecipe, materials::prototype_hash, validate};

    fn anchor() -> DecorationAnchor {
        DecorationAnchor {
            id: "bench-by-window".into(),
            label: "Bench".into(),
            position: [1., 2., 0.],
            normal: [0., 0., 1.],
            twist_degrees: 0.,
        }
    }

    #[test]
    fn decoration_anchor_uses_mount_normal_and_right_handed_twist() {
        let mut anchor = anchor();
        assert_eq!(anchor.forward(), Vec3::Y.to_array());
        anchor.twist_degrees = 90.;
        assert!(Vec3::from_array(anchor.forward()).abs_diff_eq(-Vec3::X, 1e-6));

        anchor.normal = [0., -1., 0.];
        anchor.twist_degrees = 0.;
        assert_eq!(anchor.forward(), Vec3::Z.to_array());
        anchor.twist_degrees = -90.;
        assert!(Vec3::from_array(anchor.forward()).abs_diff_eq(Vec3::X, 1e-6));

        anchor.normal = [f32::MAX, -f32::MAX, f32::MAX];
        anchor.twist_degrees = 33.;
        let normal = anchor.unit_normal().unwrap();
        let forward = Vec3::from_array(anchor.forward());
        assert!(normal.is_normalized());
        assert!(forward.is_normalized());
        assert!(normal.dot(forward).abs() < 1e-6);
    }

    #[test]
    fn decoration_anchor_round_trip_preserves_legacy_and_tracks_identity() {
        let legacy: BuildingRecipe = serde_json::from_str(include_str!("../recipes/hut.json"))
            .expect("checked-in hut recipe");
        assert!(legacy.decoration_anchors.is_empty());
        let legacy_hash = prototype_hash(&legacy, &[]).unwrap();
        let mut recipe = legacy;
        let mut json = serde_json::to_value(&recipe).unwrap();
        assert!(json.get("decoration_anchors").is_none());
        json["decoration_anchors"] = serde_json::json!([]);
        let empty = serde_json::from_value(json).unwrap();
        assert_eq!(prototype_hash(&empty, &[]).unwrap(), legacy_hash);

        recipe.decoration_anchors.push(anchor());
        let json = serde_json::to_string(&recipe).unwrap();
        assert_eq!(
            serde_json::from_str::<BuildingRecipe>(&json).unwrap(),
            recipe
        );
        let authored_hash = prototype_hash(&recipe, &[]).unwrap();
        assert_ne!(authored_hash, legacy_hash);
        recipe.decoration_anchors[0].twist_degrees = 35.;
        assert_ne!(prototype_hash(&recipe, &[]).unwrap(), authored_hash);
    }

    #[test]
    fn decoration_anchor_validation_checks_ids_labels_and_pose() {
        let mut recipe = BuildingRecipe::hut();
        recipe.decoration_anchors.push(anchor());
        validate(&recipe).unwrap();
        recipe.decoration_anchors.push(DecorationAnchor {
            label: " ".into(),
            position: [f32::INFINITY, 0., 0.],
            normal: [0.; 3],
            twist_degrees: f32::NAN,
            ..anchor()
        });
        let diagnostics = validate(&recipe).unwrap_err().diagnostics;
        for text in ["ID", "label", "position", "normal", "twist"] {
            assert!(diagnostics.iter().any(|item| item.message.contains(text)));
        }
        recipe.decoration_anchors.pop();
        recipe.decoration_anchors[0].label = "a".repeat(129);
        assert!(validate(&recipe).is_err());
    }
}
