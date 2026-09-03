using UnityEngine;

[CreateAssetMenu(
    fileName = "IslandConfiguration",
    menuName = "Motu/Island Configuration")]
public sealed class IslandConfiguration : ScriptableObject
{
    [Header("Lifecycle and Generation")]
    [SerializeField] private IslandGenerationSettings generation = new IslandGenerationSettings();

    [Header("Rivers")]
    [SerializeField] private IslandRiverSettings rivers = new IslandRiverSettings();

    [Header("Forest")]
    [SerializeField] private IslandForestSettings forest = new IslandForestSettings();

    [Header("Riverbank Reeds and Rushes")]
    [SerializeField] private IslandReedSettings reeds = new IslandReedSettings();

    [Header("Tree Trunk Ferns")]
    [SerializeField] private IslandFernSettings ferns = new IslandFernSettings();

    [Header("Clouds")]
    [SerializeField] private IslandCloudSettings clouds = new IslandCloudSettings();

    [Header("Rendering and Texture Overrides")]
    [SerializeField] private IslandRenderingSettings rendering = new IslandRenderingSettings();

    [Header("Decoration Asset Libraries")]
    [SerializeField] private IslandDecorationSettings decorations = new IslandDecorationSettings();

    [Header("Debug")]
    [SerializeField] private IslandDebugSettings debugSettings = new IslandDebugSettings();

    public IslandGenerationSettings Generation => generation;
    public IslandRiverSettings Rivers => rivers;
    public IslandForestSettings Forest => forest;
    public IslandReedSettings Reeds => reeds;
    public IslandFernSettings Ferns => ferns;
    public IslandCloudSettings Clouds => clouds;
    public IslandRenderingSettings Rendering => rendering;
    public IslandDecorationSettings Decorations => decorations;
    public IslandDebugSettings DebugSettings => debugSettings;
}
