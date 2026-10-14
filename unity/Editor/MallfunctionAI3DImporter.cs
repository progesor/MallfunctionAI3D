// Mallfunction AI3D - Editor-only Unity-native prefab importer.
// One copy under Assets/MallfunctionAI3D/Editor for each explicitly selected project.
// This script never changes project settings, existing prefabs or gameplay components.
#if UNITY_EDITOR
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;

public sealed class MallfunctionAI3DImporter : AssetPostprocessor
{
    private const string ImportedRoot = "Assets/MallfunctionAI3D/Imported/";
    private const string MarkerName = "AI3D_READY.txt";
    private const string ManifestName = "unity_asset_manifest.json";
    private const string ReportName = "unity_import_report.json";
    private static readonly HashSet<string> Pending = new HashSet<string>();
    private static bool importing;

    [Serializable] private sealed class SourceFile { public string path; public string sha256; }
    [Serializable] private sealed class Geometry
    {
        public string path;
        public string[] lodNames;
        public int[] triangles;
        public float[] screenHeights;
        public string collider;
    }
    [Serializable] private sealed class Maps { public string baseColor; public string normal; public string packedMask; }
    [Serializable] private sealed class MaterialInfo
    {
        public float[] baseColorFactor;
        public float normalScale = 1;
        public float occlusionStrength = 1;
    }
    [Serializable] private sealed class Manifest
    {
        public int schemaVersion;
        public string assetId, versionId, pipeline, sourceGlbSha256;
        public Geometry geometry;
        public Maps textures;
        public MaterialInfo material;
        public SourceFile[] files;
    }
    [Serializable] private sealed class ImportReport
    {
        public string status;
        public string assetId, versionId, pipeline, prefabPath, materialPath, error;
        public int[] lodTriangles;
        public int rendererCount;
        public bool sharedMaterial, colliderExists;
    }

    private static void OnPostprocessAllAssets(
        string[] importedAssets, string[] deletedAssets, string[] movedAssets, string[] movedFromAssetPaths)
    {
        foreach (var item in importedAssets)
        {
            if (item.StartsWith(ImportedRoot, StringComparison.Ordinal) &&
                item.EndsWith("/" + MarkerName, StringComparison.Ordinal))
            {
                var folder = item.Substring(0, item.Length - ("/" + MarkerName).Length);
                if (Pending.Add(folder))
                    EditorApplication.delayCall += () => { Pending.Remove(folder); ImportFolder(folder); };
            }
        }
    }

    [MenuItem("Tools/Mallfunction AI3D/Import pending assets")]
    public static void ImportAll()
    {
        string root = Path.Combine(Application.dataPath, "MallfunctionAI3D", "Imported");
        if (!Directory.Exists(root)) return;
        foreach (var marker in Directory.GetFiles(root, MarkerName, SearchOption.AllDirectories))
        {
            string folder = ToAssetPath(Path.GetDirectoryName(marker));
            ImportFolder(folder);
        }
    }

    private static string ToAssetPath(string absolute)
    {
        string assets = Path.GetFullPath(Application.dataPath).TrimEnd(Path.DirectorySeparatorChar);
        string folder = Path.GetFullPath(absolute);
        if (!folder.StartsWith(assets + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("Import path escapes Assets");
        return "Assets" + folder.Substring(assets.Length).Replace('\\', '/');
    }

    private static string SafeFile(string folder, string path)
    {
        if (string.IsNullOrEmpty(path) || path.Contains("\\") ||
            path.Split('/').Any(p => p.Length == 0 || p == "." || p == ".." ||
                p.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0))
            throw new InvalidOperationException("Invalid manifest file path: " + path);
        return folder + "/" + path;
    }

    private static string DiskPath(string assetPath)
    {
        if (!assetPath.StartsWith(ImportedRoot, StringComparison.Ordinal))
            throw new InvalidOperationException("Import path outside managed Unity folder");
        return Path.GetFullPath(Path.Combine(
            Path.GetDirectoryName(Application.dataPath),
            assetPath.Replace('/', Path.DirectorySeparatorChar)));
    }

    private static void VerifyHashes(string folder, Manifest m)
    {
        if (m.files == null || m.files.Length != 4)
            throw new InvalidOperationException("Expected exactly FBX, BaseColor, Normal and Mask");
        var required = new HashSet<string> {
            m.geometry.path, m.textures.baseColor, m.textures.normal, m.textures.packedMask
        };
        foreach (var item in m.files)
        {
            if (!required.Remove(item.path)) throw new InvalidOperationException("Duplicate or unknown input: " + item.path);
            string path = DiskPath(SafeFile(folder, item.path));
            if (!File.Exists(path)) throw new FileNotFoundException("Asset file missing", path);
            string digest;
            using (var sha = SHA256.Create())
                digest = BitConverter.ToString(sha.ComputeHash(File.ReadAllBytes(path))).Replace("-", "").ToLowerInvariant();
            if (!string.Equals(digest, item.sha256, StringComparison.OrdinalIgnoreCase))
                throw new InvalidOperationException("SHA256 mismatch: " + item.path);
        }
        if (required.Count != 0) throw new InvalidOperationException("Missing source file checksums");
    }

    private static void ConfigureTexture(string path, bool normal, bool srgb)
    {
        AssetDatabase.ImportAsset(path, ImportAssetOptions.ForceSynchronousImport);
        var ti = AssetImporter.GetAtPath(path) as TextureImporter;
        if (ti == null) throw new InvalidOperationException("Unity TextureImporter missing for " + path);
        ti.textureType = normal ? TextureImporterType.NormalMap : TextureImporterType.Default;
        ti.sRGBTexture = srgb;
        ti.alphaSource = TextureImporterAlphaSource.FromInput;
        ti.alphaIsTransparency = false;
        ti.mipmapEnabled = true;
        ti.SaveAndReimport();
    }

    private static void ConfigureMesh(string path)
    {
        AssetDatabase.ImportAsset(path, ImportAssetOptions.ForceSynchronousImport);
        var mi = AssetImporter.GetAtPath(path) as ModelImporter;
        if (mi == null) throw new InvalidOperationException("Unity ModelImporter missing for FBX");
        mi.importMaterials = false;
        mi.importAnimation = false;
        mi.importCameras = false;
        mi.importLights = false;
        mi.importNormals = ModelImporterNormals.Import;
        mi.importTangents = ModelImporterTangents.CalculateMikk;
        mi.preserveHierarchy = true;
        mi.globalScale = 1f;
        mi.SaveAndReimport();
    }

    private static void CheckPipeline(string expected)
    {
        var rp = GraphicsSettings.currentRenderPipeline;
        if (rp == null) throw new InvalidOperationException("Unity project has no active URP/HDRP pipeline");
        string name = rp.GetType().FullName ?? rp.GetType().Name;
        bool match = expected == "URP" ? name.Contains("UniversalRenderPipeline") :
                     expected == "HDRP" && (name.Contains("HDRenderPipeline") || name.Contains("HighDefinition"));
        if (!match) throw new InvalidOperationException("Active pipeline mismatch: expected " + expected + ", found " + name);
    }

    private static Material MakeMaterial(string folder, Manifest m)
    {
        bool urp = m.pipeline == "URP";
        string shaderName = urp ? "Universal Render Pipeline/Lit" : "HDRP/Lit";
        var shader = Shader.Find(shaderName);
        if (shader == null) throw new InvalidOperationException("Unity shader unavailable: " + shaderName);
        string dir = folder + "/Materials";
        Directory.CreateDirectory(DiskPath(dir));
        AssetDatabase.Refresh(ImportAssetOptions.ForceSynchronousImport);
        string path = dir + "/" + m.assetId + "_" + m.pipeline + ".mat";
        if (AssetDatabase.LoadAssetAtPath<Material>(path) != null || File.Exists(DiskPath(path)))
            throw new InvalidOperationException("Existing material will not be overwritten: " + path);

        var material = new Material(shader) { name = m.assetId + "_" + m.pipeline };
        Texture2D color = AssetDatabase.LoadAssetAtPath<Texture2D>(SafeFile(folder, m.textures.baseColor));
        Texture2D normal = AssetDatabase.LoadAssetAtPath<Texture2D>(SafeFile(folder, m.textures.normal));
        Texture2D mask = AssetDatabase.LoadAssetAtPath<Texture2D>(SafeFile(folder, m.textures.packedMask));
        if (!color || !normal || !mask) throw new InvalidOperationException("Unity imported texture is missing");
        float[] f = m.material != null && m.material.baseColorFactor != null && m.material.baseColorFactor.Length == 4
            ? m.material.baseColorFactor : new[] {1f,1f,1f,1f};
        Color tint = new Color(f[0], f[1], f[2], f[3]);
        if (urp)
        {
            material.SetTexture("_BaseMap", color);
            material.SetColor("_BaseColor", tint);
            material.SetTexture("_BumpMap", normal);
            material.SetFloat("_BumpScale", m.material != null ? m.material.normalScale : 1f);
            // Stock URP Lit reads R from metallic map, G from occlusion map and A from metallic map.
            // Reuse the SAME linear RGBA PNG in both slots; URP Lit has no generic _MaskMap.
            material.SetTexture("_MetallicGlossMap", mask);
            material.SetTexture("_OcclusionMap", mask);
            material.SetFloat("_Metallic", 1f);
            material.SetFloat("_Smoothness", 1f);
            material.SetFloat("_SmoothnessTextureChannel", 0f);
            material.SetFloat("_OcclusionStrength", m.material != null ? m.material.occlusionStrength : 1f);
            material.EnableKeyword("_METALLICSPECGLOSSMAP");
            material.EnableKeyword("_OCCLUSIONMAP");
            material.EnableKeyword("_NORMALMAP");
        }
        else
        {
            material.SetTexture("_BaseColorMap", color);
            material.SetColor("_BaseColor", tint);
            material.SetTexture("_NormalMap", normal);
            material.SetFloat("_NormalScale", m.material != null ? m.material.normalScale : 1f);
            material.SetTexture("_MaskMap", mask);
            material.SetFloat("_Metallic", 1f);
            // HDRP AO strength remap (never multiply the exported source twice).
            float strength = Mathf.Clamp01(m.material != null ? m.material.occlusionStrength : 1f);
            material.SetFloat("_AORemapMin", 1f - strength);
            material.SetFloat("_AORemapMax", 1f);
            material.EnableKeyword("_MASKMAP");
            material.EnableKeyword("_NORMALMAP_TANGENT_SPACE");
        }
        AssetDatabase.CreateAsset(material, path);
        return material;
    }

    private static ImportReport MakePrefab(string folder, Manifest m, Material mat)
    {
        string meshPath = SafeFile(folder, m.geometry.path);
        var model = AssetDatabase.LoadAssetAtPath<GameObject>(meshPath);
        if (!model) throw new InvalidOperationException("Unity did not load FBX model: " + meshPath);
        var root = new GameObject(m.assetId);
        try
        {
            var instance = PrefabUtility.InstantiatePrefab(model) as GameObject;
            if (!instance) throw new InvalidOperationException("Cannot instantiate FBX");
            instance.transform.SetParent(root.transform, false);
            Renderer[][] renderers = new Renderer[3][];
            var triangles = new int[3];
            for (int i = 0; i < 3; i++)
            {
                string expected = "LOD" + i + "_Mesh";
                var matches = instance.GetComponentsInChildren<MeshRenderer>(true)
                    .Where(r => r.gameObject.name == expected).ToArray();
                if (matches.Length != 1) throw new InvalidOperationException("FBX must have one renderer " + expected);
                var mr = matches[0];
                mr.sharedMaterial = mat;
                var filter = mr.GetComponent<MeshFilter>();
                if (!filter || !filter.sharedMesh) throw new InvalidOperationException("LOD missing mesh: " + expected);
                var mesh = filter.sharedMesh;
                triangles[i] = mesh.triangles.Length / 3;
                if (mesh.uv == null || mesh.uv.Length == 0 ||
                    mesh.normals == null || mesh.normals.Length == 0)
                    throw new InvalidOperationException("LOD missing UV or normals: " + expected);
                if (triangles[i] != m.geometry.triangles[i])
                    throw new InvalidOperationException("Triangle count mismatch: " + expected);
                renderers[i] = new Renderer[] { mr };
            }
            var group = root.AddComponent<LODGroup>();
            var lods = new LOD[3];
            for (int i = 0; i < 3; i++)
                lods[i] = new LOD(m.geometry.screenHeights[i], renderers[i]);
            group.SetLODs(lods);
            group.RecalculateBounds();

            if (m.geometry.collider == "Box")
            {
                Bounds b = renderers[0][0].bounds;
                var collider = root.AddComponent<BoxCollider>();
                collider.center = root.transform.InverseTransformPoint(b.center);
                collider.size = b.size;
            }
            else if (m.geometry.collider != "None")
                throw new InvalidOperationException("Unsupported collider: " + m.geometry.collider);

            string dir = folder + "/Prefabs";
            Directory.CreateDirectory(DiskPath(dir));
            AssetDatabase.Refresh(ImportAssetOptions.ForceSynchronousImport);
            string prefabPath = dir + "/" + m.assetId + ".prefab";
            if (File.Exists(DiskPath(prefabPath)))
                throw new InvalidOperationException("Existing prefab will not be overwritten: " + prefabPath);
            bool saved;
            PrefabUtility.SaveAsPrefabAsset(root, prefabPath, out saved);
            if (!saved) throw new InvalidOperationException("Unity prefab save failed");
            var prefab = AssetDatabase.LoadAssetAtPath<GameObject>(prefabPath);
            if (!prefab || prefab.GetComponent<LODGroup>() == null)
                throw new InvalidOperationException("Saved prefab could not be reloaded");
            var all = prefab.GetComponentsInChildren<MeshRenderer>(true);
            if (all.Length != 3 || all.Any(r => r.sharedMaterial != mat))
                throw new InvalidOperationException("Shared material or renderer acceptance failed");
            if (m.geometry.collider == "Box" && !prefab.GetComponent<BoxCollider>())
                throw new InvalidOperationException("Saved collider acceptance failed");
            return new ImportReport { status = "UnityPrefabReady", assetId = m.assetId,
                versionId = m.versionId, pipeline = m.pipeline, prefabPath = prefabPath,
                materialPath = AssetDatabase.GetAssetPath(mat), lodTriangles = triangles,
                rendererCount = all.Length, sharedMaterial = true,
                colliderExists = prefab.GetComponent<Collider>() != null };
        }
        finally { UnityEngine.Object.DestroyImmediate(root); }
    }

    private static void ImportFolder(string folder)
    {
        if (importing || !folder.StartsWith(ImportedRoot, StringComparison.Ordinal)) return;
        string reportPath = folder + "/" + ReportName;
        if (File.Exists(DiskPath(reportPath))) return; // Immutable/versioned; no silent rebuild.
        if (!File.Exists(DiskPath(folder + "/" + MarkerName))) return;
        importing = true;
        ImportReport report = new ImportReport {status = "ImportFailed"};
        try
        {
            var m = JsonUtility.FromJson<Manifest>(File.ReadAllText(DiskPath(folder + "/" + ManifestName)));
            if (m == null || m.schemaVersion != 2 ||
                string.IsNullOrEmpty(m.assetId) || string.IsNullOrEmpty(m.versionId) ||
                (m.pipeline != "URP" && m.pipeline != "HDRP") ||
                m.geometry == null || m.textures == null ||
                m.geometry.triangles == null || m.geometry.triangles.Length != 3 ||
                m.geometry.screenHeights == null || m.geometry.screenHeights.Length != 3)
                throw new InvalidOperationException("Invalid AI3D manifest schema");
            report.assetId = m.assetId; report.versionId = m.versionId; report.pipeline = m.pipeline;
            CheckPipeline(m.pipeline);
            VerifyHashes(folder, m);
            ConfigureTexture(SafeFile(folder, m.textures.baseColor), false, true);
            ConfigureTexture(SafeFile(folder, m.textures.normal), true, false);
            ConfigureTexture(SafeFile(folder, m.textures.packedMask), false, false);
            ConfigureMesh(SafeFile(folder, m.geometry.path));
            var mat = MakeMaterial(folder, m);
            report = MakePrefab(folder, m, mat);
            Debug.Log("[AI3D] UnityPrefabReady: " + report.prefabPath);
        }
        catch (Exception e)
        {
            report.error = e.ToString();
            Debug.LogError("[AI3D] Import failed: " + folder + "\n" + e);
        }
        finally
        {
            // Write an explicit completion report; the Studio never calls staged files "imported".
            File.WriteAllText(DiskPath(reportPath), JsonUtility.ToJson(report, true));
            AssetDatabase.Refresh(ImportAssetOptions.ForceSynchronousImport);
            importing = false;
        }
    }
}
#endif
