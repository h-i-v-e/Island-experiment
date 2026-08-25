using UnityEditor;

/// <summary>
/// Forwards the established terrain-bake menu item to the JSON-backed
/// Procedural Material Studio.
/// </summary>
public sealed class ProceduralMaterialStudioMenu : EditorWindow
{
    [MenuItem("Island/Terrain/Bake Procedural Textures")]
    private static void OpenProceduralMaterialStudio()
    {
        ProceduralMaterialEditorWindow.OpenWindow();
    }

    [MenuItem("Island/Terrain/Configure Generated Texture Imports")]
    public static void ConfigureCommittedGeneratedTextures()
    {
        ProceduralMaterialEditorWindow.ConfigureCommittedGeneratedTextures();
    }
}
