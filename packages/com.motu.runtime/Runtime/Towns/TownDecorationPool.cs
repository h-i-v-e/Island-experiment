using System.Collections.Generic;
using UnityEngine;
using UnityEngine.SceneManagement;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Towns
{
    internal sealed class TownDecorationPool
    {
        private static readonly Dictionary<Scene, TownDecorationPool> pools = new();

        private readonly Scene scene;
        private readonly GameObject root;
        private readonly Dictionary<GameObject, Stack<GameObject>> available = new();
        private int owners;

        private TownDecorationPool(Scene scene)
        {
            this.scene = scene;
            root = new GameObject("Town decoration pool");
            root.SetActive(false);
            SceneManager.MoveGameObjectToScene(root, scene);
        }

        internal static TownDecorationPool Acquire(Scene scene)
        {
            if (!pools.TryGetValue(scene, out var pool))
            {
                pool = new TownDecorationPool(scene);
                pools.Add(scene, pool);
            }
            pool.owners++;
            return pool;
        }

        internal GameObject Rent(GameObject prefab)
        {
            if (available.TryGetValue(prefab, out var idle))
            {
                while (idle.Count != 0)
                {
                    var instance = idle.Pop();
                    if (instance != null) return instance;
                }
            }
            var spawned = Object.Instantiate(prefab, root.transform, false);
            spawned.SetActive(false);
            return spawned;
        }

        internal void Return(GameObject prefab, GameObject instance)
        {
            if (instance == null) return;
            instance.SetActive(false);
            if (root == null)
            {
                DestroyUnityObject(instance);
                return;
            }
            instance.transform.SetParent(root.transform, false);
            if (!available.TryGetValue(prefab, out var idle))
            {
                idle = new Stack<GameObject>();
                available.Add(prefab, idle);
            }
            idle.Push(instance);
        }

        internal void Release()
        {
            if (--owners != 0) return;
            pools.Remove(scene);
            DestroyUnityObject(root);
        }
    }
}
