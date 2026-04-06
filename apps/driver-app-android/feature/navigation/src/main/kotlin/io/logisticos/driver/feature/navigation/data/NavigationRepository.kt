package io.logisticos.driver.feature.navigation.data

import io.logisticos.driver.core.database.dao.RouteDao
import io.logisticos.driver.core.database.entity.RouteEntity
import io.logisticos.driver.core.network.service.DirectionsApiService
import kotlinx.coroutines.flow.Flow
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import javax.inject.Inject
import javax.inject.Named

class NavigationRepository @Inject constructor(
    private val api: DirectionsApiService,
    private val routeDao: RouteDao,
    @Named("maps_api_key") private val mapsApiKey: String
) {
    /**
     * Observes the cached route for [taskId] from Room as a reactive Flow.
     * Room re-emits automatically whenever the routes table is updated for this taskId.
     */
    fun observeRoute(taskId: String): Flow<RouteEntity?> = routeDao.getByTaskId(taskId)

    suspend fun fetchRoute(
        taskId: String,
        originLat: Double,
        originLng: Double,
        destLat: Double,
        destLng: Double
    ) {
        val response = api.getDirections(
            origin = "$originLat,$originLng",
            destination = "$destLat,$destLng",
            apiKey = mapsApiKey
        )
        val route = response.routes.firstOrNull() ?: return
        val leg = route.legs.firstOrNull()
        routeDao.insert(
            RouteEntity(
                taskId = taskId,
                polylineEncoded = route.overviewPolyline.points,
                distanceMeters = leg?.distance?.value ?: 0,
                durationSeconds = leg?.duration?.value ?: 0,
                stepsJson = Json.encodeToString(leg?.steps ?: emptyList()),
                etaTimestamp = System.currentTimeMillis() +
                        (leg?.duration?.value?.toLong() ?: 0L) * 1_000L,
                fetchedAt = System.currentTimeMillis()
            )
        )
    }
}
