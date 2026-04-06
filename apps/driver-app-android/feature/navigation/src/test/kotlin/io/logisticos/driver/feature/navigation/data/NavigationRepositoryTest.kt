package io.logisticos.driver.feature.navigation.data

import io.logisticos.driver.core.database.dao.RouteDao
import io.logisticos.driver.core.network.service.DirectionsApiService
import io.logisticos.driver.core.network.service.DirectionsLatLng
import io.logisticos.driver.core.network.service.DirectionsLeg
import io.logisticos.driver.core.network.service.DirectionsResponse
import io.logisticos.driver.core.network.service.DirectionsRoute
import io.logisticos.driver.core.network.service.DirectionsStep
import io.logisticos.driver.core.network.service.OverviewPolyline
import io.logisticos.driver.core.network.service.TextValue
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test

class NavigationRepositoryTest {

    private val api: DirectionsApiService = mockk()
    private val routeDao: RouteDao = mockk(relaxed = true)
    private val repo = NavigationRepository(api, routeDao, mapsApiKey = "test_key")

    @Test
    fun `fetchRoute stores route in room when response has routes`() = runTest {
        coEvery {
            api.getDirections(any(), any(), any(), any(), any())
        } returns DirectionsResponse(
            routes = listOf(
                DirectionsRoute(
                    overviewPolyline = OverviewPolyline("encoded_polyline"),
                    legs = listOf(
                        DirectionsLeg(
                            duration = TextValue("10 mins", 600),
                            distance = TextValue("5 km", 5000),
                            steps = listOf(
                                DirectionsStep(
                                    htmlInstructions = "Turn right",
                                    distance = TextValue("500 m", 500),
                                    duration = TextValue("2 mins", 120),
                                    endLocation = DirectionsLatLng(14.56, 121.04)
                                )
                            )
                        )
                    )
                )
            ),
            status = "OK"
        )

        repo.fetchRoute(
            taskId = "t1",
            originLat = 14.55,
            originLng = 121.03,
            destLat = 14.60,
            destLng = 121.05
        )

        coVerify { routeDao.insert(any()) }
    }

    @Test
    fun `fetchRoute does nothing when response has no routes`() = runTest {
        coEvery {
            api.getDirections(any(), any(), any(), any(), any())
        } returns DirectionsResponse(routes = emptyList(), status = "ZERO_RESULTS")

        repo.fetchRoute(
            taskId = "t2",
            originLat = 14.55,
            originLng = 121.03,
            destLat = 14.60,
            destLng = 121.05
        )

        coVerify(exactly = 0) { routeDao.insert(any()) }
    }
}
